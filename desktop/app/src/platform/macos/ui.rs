//! The AppKit side: the windows, and what's read from them. Everything here
//! runs on the main thread; the overlay thread queues work with `on_main`.
//!
//! Levels, lowest first: toolbar, overlay.

use std::sync::{Mutex, OnceLock};

use objc2::ffi::{object_setClass, NSInteger, NSUInteger};
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};

use super::ffi::*;

const SCREEN_SAVER: NSInteger = 1000;
const LEVEL_TOOLBAR: NSInteger = SCREEN_SAVER + 1;
const LEVEL_OVERLAY: NSInteger = SCREEN_SAVER + 2;

const JOIN_ALL_SPACES: NSUInteger = 1 << 0;
const STATIONARY: NSUInteger = 1 << 4;
const IGNORES_CYCLE: NSUInteger = 1 << 6;
const FULL_SCREEN_AUX: NSUInteger = 1 << 8;
const BEHAVIOR: NSUInteger = JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUX;

const NONACTIVATING: NSUInteger = 1 << 7;
const BORDERLESS: NSUInteger = 0;
const BACKING_BUFFERED: NSUInteger = 2;

/// A raw object pointer, for keeping in a static. Only touched on the main thread.
#[derive(Clone, Copy, PartialEq)]
struct Obj(*mut AnyObject);
unsafe impl Send for Obj {}

impl Obj {
    fn get(self) -> &'static AnyObject {
        unsafe { &*self.0 }
    }
}

#[derive(Clone, Copy)]
struct Sheet {
    window: Obj,
    layer: Obj,
}

#[derive(Default)]
struct Windows {
    overlay: Option<Sheet>,
    toolbar: Option<Obj>,
}

fn windows() -> &'static Mutex<Windows> {
    static W: OnceLock<Mutex<Windows>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(Windows::default()))
}

/// AppKit puts the origin at the bottom left of the primary screen; the rest
/// of this code uses CoreGraphics' top left.
fn primary_height() -> f64 {
    unsafe { CGDisplayBounds(CGMainDisplayID()).size.height }
}

fn flip(r: CGRect) -> CGRect {
    CGRect::new(r.origin.x, primary_height() - r.origin.y - r.size.height, r.size.width, r.size.height)
}

fn make_sheet(level: NSInteger) -> Sheet {
    unsafe {
        let alloc: *mut AnyObject = msg_send![class!(NSWindow), alloc];
        let window: *mut AnyObject = msg_send![
            alloc,
            initWithContentRect: CGRect::new(0.0, 0.0, 10.0, 10.0),
            styleMask: BORDERLESS,
            backing: BACKING_BUFFERED,
            defer: Bool::NO
        ];
        let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![window, setOpaque: Bool::NO];
        let _: () = msg_send![window, setBackgroundColor: clear];
        let _: () = msg_send![window, setHasShadow: Bool::NO];
        let _: () = msg_send![window, setIgnoresMouseEvents: Bool::YES];
        let _: () = msg_send![window, setReleasedWhenClosed: Bool::NO];
        let _: () = msg_send![window, setLevel: level];
        let _: () = msg_send![window, setCollectionBehavior: BEHAVIOR];
        let layer: *mut AnyObject = msg_send![class!(CALayer), layer];
        let _: *mut AnyObject = msg_send![layer, retain];
        let _: () = msg_send![layer, setOpaque: Bool::NO];
        let view: *mut AnyObject = msg_send![window, contentView];
        let _: () = msg_send![view, setLayer: layer];
        let _: () = msg_send![view, setWantsLayer: Bool::YES];
        Sheet { window: Obj(window), layer: Obj(layer) }
    }
}

fn set_frame(sheet: Sheet, frame: CGRect) {
    unsafe {
        let _: () = msg_send![sheet.window.get(), setFrame: flip(frame), display: Bool::NO];
    }
}

fn front(sheet: Sheet) {
    unsafe {
        let _: () = msg_send![sheet.window.get(), orderFrontRegardless];
    }
}

fn hide(sheet: Sheet) {
    unsafe {
        let _: () = msg_send![sheet.window.get(), orderOut: std::ptr::null_mut::<AnyObject>()];
    }
}

/// Swap a layer's picture with no implicit fade.
fn set_contents(sheet: Sheet, contents: Ptr) {
    unsafe {
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: Bool::YES];
        let _: () = msg_send![sheet.layer.get(), setContents: contents as *mut AnyObject];
        let _: () = msg_send![class!(CATransaction), commit];
    }
}

fn overlay_sheet() -> Sheet {
    let mut w = windows().lock().unwrap();
    *w.overlay.get_or_insert_with(|| make_sheet(LEVEL_OVERLAY))
}

pub fn overlay_show(frame: CGRect) {
    let sheet = overlay_sheet();
    set_frame(sheet, frame);
    front(sheet);
    let w = windows().lock().unwrap();
    if let Some(t) = w.toolbar {
        unsafe {
            let _: () = msg_send![t.get(), orderFrontRegardless];
        }
    }
}

pub fn overlay_present(surface: usize) {
    set_contents(overlay_sheet(), surface as Ptr);
}

/// Over the Dock the window takes the mouse, so the Dock can't set the cursor.
pub fn overlay_catch_mouse(catch: bool) {
    let sheet = overlay_sheet();
    unsafe {
        let _: () = msg_send![sheet.window.get(), setIgnoresMouseEvents: Bool::from(!catch)];
    }
}

pub fn hide_all() {
    let w = windows().lock().unwrap();
    for sheet in [w.overlay].into_iter().flatten() {
        hide(sheet);
    }
    if let Some(t) = w.toolbar {
        unsafe {
            let _: () = msg_send![t.get(), orderOut: std::ptr::null_mut::<AnyObject>()];
        }
    }
}

extern "C-unwind" fn never(_: &AnyObject, _: Sel) -> Bool {
    Bool::NO
}

/// A panel class: clicks on it must not pull focus from the app being
/// presented, and only panels can avoid that.
fn panel_class() -> &'static AnyClass {
    static C: OnceLock<usize> = OnceLock::new();
    let addr = *C.get_or_init(|| unsafe {
        let superclass = class!(NSPanel);
        let mut b = ClassBuilder::new(c"TinyHandPanel", superclass).expect("panel class");
        b.add_method(sel!(canBecomeKeyWindow), never as extern "C-unwind" fn(_, _) -> _);
        b.add_method(sel!(canBecomeMainWindow), never as extern "C-unwind" fn(_, _) -> _);
        b.register() as *const AnyClass as usize
    });
    unsafe { &*(addr as *const AnyClass) }
}

pub fn prepare_toolbar(window: Ptr) {
    let obj = Obj(window as *mut AnyObject);
    unsafe {
        object_setClass(obj.0 as *mut _, panel_class() as *const AnyClass as *const _);
        let mask: NSUInteger = msg_send![obj.get(), styleMask];
        let _: () = msg_send![obj.get(), setStyleMask: mask | NONACTIVATING];
        let _: () = msg_send![obj.get(), setHidesOnDeactivate: Bool::NO];
        let _: () = msg_send![obj.get(), setBecomesKeyOnlyIfNeeded: Bool::YES];
        let _: () = msg_send![obj.get(), setLevel: LEVEL_TOOLBAR];
        let _: () = msg_send![obj.get(), setCollectionBehavior: BEHAVIOR];
    }
    windows().lock().unwrap().toolbar = Some(obj);
}

/// Keep the toolbar centred just above the bottom of `visible`.
pub fn place_toolbar(visible: CGRect) {
    let Some(t) = windows().lock().unwrap().toolbar else { return };
    unsafe {
        let frame: CGRect = msg_send![t.get(), frame];
        let x = visible.origin.x + (visible.size.width - frame.size.width) / 2.0;
        let bottom = primary_height() - (visible.origin.y + visible.size.height) + 2.0;
        if (frame.origin.x - x).abs() > 0.5 || (frame.origin.y - bottom).abs() > 0.5 {
            let _: () = msg_send![t.get(), setFrameOrigin: CGPoint { x, y: bottom }];
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct ScreenInfo {
    pub frame: CGRect,
    pub visible: CGRect,
    pub scale: f64,
}

#[derive(Clone, Default)]
pub struct Geometry {
    pub screens: Vec<ScreenInfo>,
    pub toolbar: Option<CGRect>,
    /// Our other windows that are up (Settings): clicks go to them.
    pub own_windows: Vec<CGRect>,
}

pub fn geometry() -> &'static Mutex<Geometry> {
    static G: OnceLock<Mutex<Geometry>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(Geometry::default()))
}

/// Refresh what the overlay thread knows about screens and windows.
pub fn refresh_geometry() {
    autoreleasepool(|_| unsafe {
        let mut g = Geometry::default();
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        let n: NSUInteger = msg_send![screens, count];
        for i in 0..n {
            let s: *mut AnyObject = msg_send![screens, objectAtIndex: i];
            let frame: CGRect = msg_send![s, frame];
            let visible: CGRect = msg_send![s, visibleFrame];
            let scale: f64 = msg_send![s, backingScaleFactor];
            g.screens.push(ScreenInfo { frame: flip(frame), visible: flip(visible), scale });
        }
        let w = windows().lock().unwrap();
        let ours: Vec<Obj> = [w.overlay].into_iter().flatten().map(|s| s.window).collect();
        if let Some(t) = w.toolbar {
            let frame: CGRect = msg_send![t.get(), frame];
            g.toolbar = Some(flip(frame));
        }
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let list: *mut AnyObject = msg_send![app, windows];
        let n: NSUInteger = msg_send![list, count];
        for i in 0..n {
            let win: *mut AnyObject = msg_send![list, objectAtIndex: i];
            if ours.contains(&Obj(win)) || w.toolbar == Some(Obj(win)) {
                continue;
            }
            let visible: Bool = msg_send![win, isVisible];
            let level: NSInteger = msg_send![win, level];
            // Normal level: Settings. Skips the status item's own window.
            if visible.as_bool() && level == 0 {
                let frame: CGRect = msg_send![win, frame];
                g.own_windows.push(flip(frame));
            }
        }
        *geometry().lock().unwrap() = g;
    });
}
