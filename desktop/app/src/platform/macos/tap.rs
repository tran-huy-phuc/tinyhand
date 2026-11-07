//! The event tap: the macOS twin of the Windows mouse hook, and a port of
//! `InputTap.swift`. While the pointer is on, presses are swallowed and
//! reported as taps, and drags reach other apps as plain moves.
//!
//! Needs the Accessibility permission. Without it `install` returns nothing
//! and the overlay keeps retrying while clicks simply go through.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use super::ffi::*;
use super::overlay::Msg;

pub enum TapEvent {
    Moved,
    Down { left: bool },
    Up,
}

pub struct Shared {
    pub active: AtomicBool,
    /// Flag of the key that lets clicks through while held; 0 = off.
    pub interact_flag: AtomicU64,
    /// Global rects (points, top-left origin) where presses go through.
    pub pass: Mutex<Vec<CGRect>>,
    pub idle: AtomicBool,
    /// A press is going through because of the interact key.
    pub interact_press: AtomicBool,
    passing: AtomicBool,
    tx: Mutex<Sender<Msg>>,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
static TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

impl Shared {
    pub fn new(tx: Sender<Msg>) -> Arc<Shared> {
        Arc::new(Shared {
            active: AtomicBool::new(false),
            interact_flag: AtomicU64::new(0),
            pass: Mutex::new(Vec::new()),
            idle: AtomicBool::new(false),
            interact_press: AtomicBool::new(false),
            passing: AtomicBool::new(false),
            tx: Mutex::new(tx),
        })
    }

    pub fn reset(&self) {
        self.interact_press.store(false, Ordering::Relaxed);
        self.passing.store(false, Ordering::Relaxed);
    }

    fn send(&self, e: TapEvent) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Msg::Tap(e));
        }
    }

    /// Returns the event to deliver, or null to swallow it.
    unsafe fn handle(&self, kind: u32, event: Ptr) -> Ptr {
        if kind == TAP_DISABLED_TIMEOUT || kind == TAP_DISABLED_INPUT {
            if self.active.load(Ordering::Relaxed) {
                CGEventTapEnable(TAP.load(Ordering::Relaxed), true);
            }
            return event;
        }
        if !self.active.load(Ordering::Relaxed) {
            return event;
        }
        let is_down = matches!(kind, LEFT_DOWN | RIGHT_DOWN | OTHER_DOWN);
        let is_up = matches!(kind, LEFT_UP | RIGHT_UP | OTHER_UP);
        let is_drag = matches!(kind, LEFT_DRAGGED | RIGHT_DRAGGED | OTHER_DRAGGED);

        if kind == MOVED {
            if self.idle.swap(false, Ordering::Relaxed) {
                self.send(TapEvent::Moved);
            }
            return event;
        }

        let key = self.interact_flag.load(Ordering::Relaxed);
        if is_down && key != 0 && (CGEventGetFlags(event) & key != 0 || CGEventSourceFlagsState(SOURCE_HID) & key != 0) {
            self.interact_press.store(true, Ordering::Relaxed);
        }
        if self.interact_press.load(Ordering::Relaxed) {
            // So Option+click is a plain click to the app underneath.
            CGEventSetFlags(event, CGEventGetFlags(event) & !key);
            if is_up {
                self.interact_press.store(false, Ordering::Relaxed);
            }
            return event;
        }

        if is_down {
            let at = CGEventGetLocation(event);
            let pass = self.pass.lock().map(|r| r.iter().any(|r| r.contains(at))).unwrap_or(false);
            self.passing.store(pass, Ordering::Relaxed);
        }
        if self.passing.load(Ordering::Relaxed) {
            if is_up {
                self.passing.store(false, Ordering::Relaxed);
            }
            return event;
        }

        if is_down {
            self.send(TapEvent::Down { left: kind == LEFT_DOWN });
            return std::ptr::null_mut();
        }
        if is_up {
            self.send(TapEvent::Up);
            return std::ptr::null_mut();
        }
        if is_drag {
            CGEventSetType(event, MOVED);
        }
        event
    }
}

unsafe extern "C" fn callback(_proxy: Ptr, kind: u32, event: Ptr, _user: Ptr) -> Ptr {
    match SHARED.get() {
        Some(shared) => shared.handle(kind, event),
        None => event,
    }
}

fn mask() -> u64 {
    [LEFT_DOWN, LEFT_UP, RIGHT_DOWN, RIGHT_UP, MOVED, LEFT_DRAGGED, RIGHT_DRAGGED, OTHER_DOWN, OTHER_UP, OTHER_DRAGGED]
        .iter()
        .fold(0, |m, k| m | (1u64 << k))
}

/// Create the tap on a thread of its own, once. Returns false until the
/// Accessibility permission has been granted.
pub fn install(shared: &Arc<Shared>) -> bool {
    if !TAP.load(Ordering::Relaxed).is_null() {
        return true;
    }
    let _ = SHARED.set(shared.clone());
    unsafe {
        let tap = CGEventTapCreate(TAP_SESSION, TAP_HEAD, TAP_DEFAULT, mask(), callback, std::ptr::null_mut());
        if tap.is_null() {
            return false;
        }
        CGEventTapEnable(tap, false);
        TAP.store(tap, Ordering::Relaxed);
        let tap_addr = tap as usize;
        std::thread::Builder::new()
            .name("tinyhand-tap".into())
            .spawn(move || {
                let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap_addr as Ptr, 0);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                CFRunLoopRun();
            })
            .is_ok()
    }
}

pub fn set_enabled(on: bool) {
    let tap = TAP.load(Ordering::Relaxed);
    if !tap.is_null() {
        unsafe { CGEventTapEnable(tap, on) };
    }
}
