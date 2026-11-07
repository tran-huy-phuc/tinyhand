//! Hiding the system cursor, a port of `CursorHider.swift`.
//!
//! `CGDisplayHideCursor` only sticks for a background app after the private
//! "SetsCursorInBackground" connection property is set, and other apps can
//! still bring the cursor back when they change it. The cursor seed tells us
//! when that happened, so we hide it again. Main thread only.
//!
//! The private calls are looked up at runtime: if a macOS release removes
//! them, the cursor just stays visible on top of the hand.

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use objc2::msg_send;
use objc2::runtime::AnyObject;

use super::ffi::*;

type MainConn = unsafe extern "C" fn() -> i32;
type SetProp = unsafe extern "C" fn(i32, i32, Ptr, Ptr) -> i32;
type Seed = unsafe extern "C" fn() -> i32;

struct Private {
    conn: Option<MainConn>,
    set_prop: Option<SetProp>,
    seed: Option<Seed>,
}

fn private() -> &'static Private {
    static P: OnceLock<Private> = OnceLock::new();
    P.get_or_init(|| unsafe {
        let h = dlopen(std::ptr::null(), RTLD_NOW);
        let find = |name: &std::ffi::CStr| -> *mut c_void {
            if h.is_null() {
                std::ptr::null_mut()
            } else {
                dlsym(h, name.as_ptr())
            }
        };
        let (c, p, s) = (find(c"CGSMainConnectionID"), find(c"CGSSetConnectionProperty"), find(c"CGSCurrentCursorSeed"));
        Private {
            conn: (!c.is_null()).then(|| std::mem::transmute::<*mut c_void, MainConn>(c)),
            set_prop: (!p.is_null()).then(|| std::mem::transmute::<*mut c_void, SetProp>(p)),
            seed: (!s.is_null()).then(|| std::mem::transmute::<*mut c_void, Seed>(s)),
        }
    })
}

pub fn enable_background_hiding() {
    let p = private();
    let (Some(conn), Some(set_prop)) = (p.conn, p.set_prop) else { return };
    unsafe {
        let key: *mut AnyObject = msg_send![objc2::class!(NSString), stringWithUTF8String: c"SetsCursorInBackground".as_ptr()];
        let yes: *mut AnyObject = msg_send![objc2::class!(NSNumber), numberWithBool: objc2::runtime::Bool::YES];
        let cid = conn();
        set_prop(cid, cid, key as Ptr, yes as Ptr);
    }
}

struct State {
    hidden: bool,
    count: u32,
    hidden_at: Instant,
    last_seed: i32,
    last_reassert: Instant,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| {
        let now = Instant::now();
        Mutex::new(State { hidden: false, count: 0, hidden_at: now, last_seed: 0, last_reassert: now })
    })
}

pub fn hide() {
    let mut s = state().lock().unwrap();
    if s.hidden {
        return;
    }
    s.hidden = true;
    s.count = 1;
    s.hidden_at = Instant::now();
    enable_background_hiding();
    unsafe { CGDisplayHideCursor(CGMainDisplayID()) };
}

pub fn show() {
    let mut s = state().lock().unwrap();
    if !s.hidden {
        return;
    }
    s.hidden = false;
    for _ in 0..s.count {
        unsafe { CGDisplayShowCursor(CGMainDisplayID()) };
    }
    s.count = 0;
}

/// Hide it again if something may have brought it back.
pub fn reassert(fighting: bool, urgent: bool) {
    let mut s = state().lock().unwrap();
    if !s.hidden {
        return;
    }
    let seed = private().seed.map(|f| unsafe { f() }).unwrap_or(0);
    let now = Instant::now();
    let interval = if urgent {
        Duration::ZERO
    } else if fighting {
        Duration::from_millis(30)
    } else if now.duration_since(s.hidden_at) < Duration::from_secs(1) {
        Duration::from_millis(50)
    } else {
        Duration::from_secs(1)
    };
    if seed == s.last_seed && now.duration_since(s.last_reassert) <= interval {
        return;
    }
    s.last_seed = seed;
    s.last_reassert = now;
    enable_background_hiding();
    unsafe { CGDisplayHideCursor(CGMainDisplayID()) };
    s.count += 1;
}
