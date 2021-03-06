#[macro_use] extern crate bitflags;

extern crate fsevent_sys as fsevent;
extern crate libc;

use fsevent::core_foundation as cf;
use fsevent::fsevent as fs;

use std::slice;
use std::mem::transmute;
use std::slice::from_raw_parts_mut;
use std::str::from_utf8;
use std::ffi::CStr;
use std::convert::AsRef;

use std::sync::mpsc::{Sender};

pub const NULL: cf::CFRef = cf::NULL;

pub struct FsEvent {
  paths: cf::CFMutableArrayRef,
  since_when: fs::FSEventStreamEventId,
  latency: cf::CFTimeInterval,
  flags: fs::FSEventStreamCreateFlags,
  sender: Sender<Event>,
}

#[derive(Debug)]
pub struct Event {
  pub event_id: u64,
  pub flag: StreamFlags,
  pub path: String,
}

pub type FsEventCallback = fn(Vec<Event>);

bitflags! {
  flags StreamFlags: u32 {
    const NONE = 0x00000000,
    const MUST_SCAN_SUBDIRS = 0x00000001,
    const USER_DROPPED = 0x00000002,
    const KERNEL_DROPPED = 0x00000004,
    const IDS_WRAPPED = 0x00000008,
    const HISTORY_DONE = 0x00000010,
    const ROOT_CHANGED = 0x00000020,
    const MOUNT = 0x00000040,
    const UNMOUNT = 0x00000080,
    const ITEM_CREATED = 0x00000100,
    const ITEM_REMOVED = 0x00000200,
    const INOTE_META_MOD = 0x00000400,
    const ITEM_RENAMED = 0x00000800,
    const ITEM_MODIFIED = 0x00001000,
    const FINDER_INFO_MOD = 0x00002000,
    const ITEM_CHANGE_OWNER = 0x00004000,
    const ITEM_XATTR_MOD = 0x00008000,
    const IS_FILE = 0x00010000,
    const IS_DIR = 0x00020000,
    const IS_SYMLIMK = 0x00040000,
  }
}

pub fn is_api_available() -> (bool, String) {
  let ma = cf::system_version_major();
  let mi = cf::system_version_minor();

  if ma == 10 && mi < 5 {
    return (false, "This version of OSX does not support the FSEvent library, cannot proceed".to_string());
  }
  return (true, "ok".to_string());
}



fn default_stream_context(info: *const FsEvent) -> fs::FSEventStreamContext {
  let ptr = info as *mut libc::c_void;
  let stream_context = fs::FSEventStreamContext{
    version: 0,
    info: ptr,
    retain: cf::NULL,
    copy_description: cf::NULL
  };

  stream_context
}

impl FsEvent {
  pub fn new(sender: Sender<Event>) -> FsEvent {
    let fsevent: FsEvent;

    unsafe {
      fsevent = FsEvent{
        paths: cf::CFArrayCreateMutable(cf::kCFAllocatorDefault, 0, &cf::kCFTypeArrayCallBacks),
        since_when: fs::kFSEventStreamEventIdSinceNow,
        latency: 0.1,
        flags: fs::kFSEventStreamCreateFlagFileEvents,
        sender: sender,
      };
    }
    fsevent
  }



  // https://github.com/thibaudgg/rb-fsevent/blob/master/ext/fsevent_watch/main.c
  pub fn append_path(&self,source: &str) {
    unsafe {
      let cf_path = cf::str_path_to_cfstring_ref(source);
      cf::CFArrayAppendValue(self.paths, cf_path);
      cf::CFRelease(cf_path);
    }
  }
  pub fn observe(&self) {
    let stream_context = default_stream_context(self);

    let cb = callback as *mut _;

    unsafe {
      let stream = fs::FSEventStreamCreate(cf::kCFAllocatorDefault,
       cb,
       &stream_context,
       self.paths,
       self.since_when,
       self.latency,
       self.flags);

      // fs::FSEventStreamShow(stream);

      fs::FSEventStreamScheduleWithRunLoop(stream,
        cf::CFRunLoopGetCurrent(),
        cf::kCFRunLoopDefaultMode);

      fs::FSEventStreamStart(stream);
      cf::CFRunLoopRun();

      fs::FSEventStreamFlushSync(stream);
      fs::FSEventStreamStop(stream);

    }
  }
}

#[allow(unused_variables)]
pub fn callback(
    stream_ref: fs::FSEventStreamRef,
    info: *mut libc::c_void,
    num_events: libc::size_t,      // size_t numEvents
    event_paths: *const *const libc::c_char, // void *eventPaths
    event_flags: *mut libc::c_void, // const FSEventStreamEventFlags eventFlags[]
    event_ids: *mut libc::c_void,  // const FSEventStreamEventId eventIds[]
  ) {
  let num = num_events as usize;
  let e_ptr = event_flags as *mut u32;
  let i_ptr = event_ids as *mut u64;
  let fs_event = info as *mut FsEvent;

  unsafe {
    let paths: &[*const libc::c_char] = std::mem::transmute(slice::from_raw_parts(event_paths, num));
    let flags = from_raw_parts_mut(e_ptr, num);
    let ids = from_raw_parts_mut(i_ptr, num);

    for p in (0..num) {
      let i = CStr::from_ptr(paths[p]).to_bytes();
      let flag: StreamFlags = StreamFlags::from_bits(flags[p] as u32)
      .expect(format!("Unable to decode StreamFlags: {}", flags[p] as u32).as_ref());

      let path = from_utf8(i).ok().expect("Invalid UTF8 string.");
      let event = Event{event_id: ids[p], flag: flag, path: path.to_string()};
      let _s = (*fs_event).sender.send(event);
    }
  }
}
