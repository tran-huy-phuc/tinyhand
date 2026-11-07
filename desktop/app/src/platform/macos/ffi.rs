//! The C APIs the overlay needs: event taps, displays, windows, IOSurface and
//! Accessibility. Declared by hand; it's a small set and they're stable.

#![allow(non_upper_case_globals, non_snake_case, clippy::missing_safety_doc)]

use std::ffi::c_void;

use objc2::encode::{Encode, Encoding};

pub type Ptr = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

impl CGRect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        CGRect { origin: CGPoint { x, y }, size: CGSize { width: w, height: h } }
    }

    pub fn contains(&self, p: CGPoint) -> bool {
        p.x >= self.origin.x && p.x < self.origin.x + self.size.width && p.y >= self.origin.y && p.y < self.origin.y + self.size.height
    }

    pub fn inset(&self, dx: f64, dy: f64) -> CGRect {
        CGRect::new(self.origin.x + dx, self.origin.y + dy, self.size.width - 2.0 * dx, self.size.height - 2.0 * dy)
    }

    pub fn is_empty(&self) -> bool {
        self.size.width <= 0.0 || self.size.height <= 0.0
    }
}

unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

pub const TAP_SESSION: u32 = 1;
pub const TAP_HEAD: u32 = 0;
pub const TAP_DEFAULT: u32 = 0;

pub const LEFT_DOWN: u32 = 1;
pub const LEFT_UP: u32 = 2;
pub const RIGHT_DOWN: u32 = 3;
pub const RIGHT_UP: u32 = 4;
pub const MOVED: u32 = 5;
pub const LEFT_DRAGGED: u32 = 6;
pub const RIGHT_DRAGGED: u32 = 7;
pub const OTHER_DOWN: u32 = 25;
pub const OTHER_UP: u32 = 26;
pub const OTHER_DRAGGED: u32 = 27;
pub const TAP_DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const TAP_DISABLED_INPUT: u32 = 0xFFFF_FFFF;

pub const FLAG_SHIFT: u64 = 0x20000;
pub const FLAG_CONTROL: u64 = 0x40000;
pub const FLAG_ALT: u64 = 0x80000;

pub const SOURCE_HID: i32 = 1;
pub const LEVEL_KEY_POPUP_MENU: i32 = 11;
pub const LIST_ON_SCREEN: u32 = 1;
pub const LIST_NO_DESKTOP_ELEMENTS: u32 = 16;

pub type TapCallback = unsafe extern "C" fn(proxy: Ptr, kind: u32, event: Ptr, user: Ptr) -> Ptr;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventTapCreate(tap: u32, place: u32, options: u32, mask: u64, callback: TapCallback, user: Ptr) -> Ptr;
    pub fn CGEventTapEnable(tap: Ptr, enable: bool);
    pub fn CGEventGetFlags(event: Ptr) -> u64;
    pub fn CGEventSetFlags(event: Ptr, flags: u64);
    pub fn CGEventSetType(event: Ptr, kind: u32);
    pub fn CGEventGetLocation(event: Ptr) -> CGPoint;
    pub fn CGEventCreate(source: Ptr) -> Ptr;
    pub fn CGEventSourceFlagsState(state: i32) -> u64;
    pub fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    pub fn CGMainDisplayID() -> u32;
    pub fn CGGetDisplaysWithPoint(point: CGPoint, max: u32, displays: *mut u32, count: *mut u32) -> i32;
    pub fn CGDisplayBounds(display: u32) -> CGRect;
    pub fn CGDisplayCopyDisplayMode(display: u32) -> Ptr;
    pub fn CGDisplayModeGetPixelWidth(mode: Ptr) -> usize;
    pub fn CGDisplayModeGetPixelHeight(mode: Ptr) -> usize;
    pub fn CGDisplayModeGetRefreshRate(mode: Ptr) -> f64;
    pub fn CGDisplayModeRelease(mode: Ptr);
    pub fn CGDisplayHideCursor(display: u32) -> i32;
    pub fn CGDisplayShowCursor(display: u32) -> i32;
    pub fn CGWindowListCopyWindowInfo(option: u32, window: u32) -> Ptr;
    pub fn CGWindowLevelForKey(key: i32) -> i32;
    pub fn CGRectMakeWithDictionaryRepresentation(dict: Ptr, rect: *mut CGRect) -> bool;
    pub static kCGWindowLayer: Ptr;
    pub static kCGWindowBounds: Ptr;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRelease(object: Ptr);
    pub fn CFArrayGetCount(array: Ptr) -> isize;
    pub fn CFArrayGetValueAtIndex(array: Ptr, index: isize) -> Ptr;
    pub fn CFDictionaryGetValue(dict: Ptr, key: Ptr) -> Ptr;
    pub fn CFNumberGetValue(number: Ptr, kind: isize, out: *mut c_void) -> bool;
    pub fn CFMachPortCreateRunLoopSource(allocator: Ptr, port: Ptr, order: isize) -> Ptr;
    pub fn CFRunLoopGetCurrent() -> Ptr;
    pub fn CFRunLoopAddSource(run_loop: Ptr, source: Ptr, mode: Ptr);
    pub fn CFRunLoopRun();
    pub static kCFRunLoopCommonModes: Ptr;
}
pub const CF_NUMBER_INT: isize = 9;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXUIElementCreateApplication(pid: i32) -> Ptr;
    pub fn AXUIElementSetMessagingTimeout(element: Ptr, seconds: f32) -> i32;
    pub fn AXUIElementCopyAttributeValue(element: Ptr, attribute: Ptr, value: *mut Ptr) -> i32;
    pub fn AXValueGetValue(value: Ptr, kind: u32, out: *mut c_void) -> bool;
}
pub const AX_VALUE_POINT: u32 = 1;
pub const AX_VALUE_SIZE: u32 = 2;

#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    pub fn IOSurfaceCreate(properties: Ptr) -> Ptr;
    pub fn IOSurfaceLock(surface: Ptr, options: u32, seed: *mut u32) -> i32;
    pub fn IOSurfaceUnlock(surface: Ptr, options: u32, seed: *mut u32) -> i32;
    pub fn IOSurfaceGetBaseAddress(surface: Ptr) -> *mut u8;
    pub fn IOSurfaceGetBytesPerRow(surface: Ptr) -> usize;
    pub fn IOSurfaceGetHeight(surface: Ptr) -> usize;
    pub static kIOSurfaceWidth: Ptr;
    pub static kIOSurfaceHeight: Ptr;
    pub static kIOSurfaceBytesPerElement: Ptr;
    pub static kIOSurfacePixelFormat: Ptr;
}

extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: unsafe extern "C" fn(*mut c_void));
    pub fn dlopen(path: *const std::ffi::c_char, mode: i32) -> Ptr;
    pub fn dlsym(handle: Ptr, name: *const std::ffi::c_char) -> Ptr;
}

pub const RTLD_NOW: i32 = 2;

unsafe extern "C" fn run_boxed(context: *mut c_void) {
    let work = Box::from_raw(context as *mut Box<dyn FnOnce() + Send>);
    work();
}

/// Run on the main thread, without waiting. AppKit must only be used there.
pub fn on_main(work: impl FnOnce() + Send + 'static) {
    let boxed: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(work));
    unsafe {
        dispatch_async_f(&_dispatch_main_q, Box::into_raw(boxed) as *mut c_void, run_boxed);
    }
}
