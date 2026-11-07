//! The overlay's thread: the macOS twin of `windows/overlay.rs`, with the
//! behaviour of `OverlayController.swift`.
//!
//! Engine work and rendering happen here, off the main thread, so a busy
//! webview can't make the hand stutter. Pixels go into two IOSurfaces, drawn
//! alternately; showing one is just pointing the layer at it, which is why
//! only the changed rectangle needs painting. Anything AppKit is queued to
//! the main thread with `on_main`.
//!
//! Positions are global points with a top-left origin, as CoreGraphics gives
//! them; the engine works in the monitor's pixels.

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use tinyhand_core::prefs::InteractKey;
use tinyhand_core::{Engine, IRect, Look, Settings};

use super::ffi::*;
use super::tap::{self, Shared, TapEvent};
use super::{cursor, ui};
use crate::log;
use crate::platform::{OverlayConfig, OverlayEvent};

pub enum Msg {
    Configure(OverlayConfig),
    Activate,
    Deactivate,
    ClearInk,
    Tap(TapEvent),
}

pub struct Overlay {
    tx: Sender<Msg>,
}

impl Overlay {
    pub fn new(on_event: impl Fn(OverlayEvent) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Shared::new(tx.clone());
        std::thread::Builder::new()
            .name("tinyhand-overlay".into())
            .spawn(move || Runner::new(rx, shared, Box::new(on_event)).run())
            .expect("overlay thread");
        Overlay { tx }
    }

    pub fn configure(&self, cfg: OverlayConfig) {
        let _ = self.tx.send(Msg::Configure(cfg));
    }

    pub fn activate(&self, _toolbar: Option<isize>) {
        let _ = self.tx.send(Msg::Activate);
    }

    pub fn deactivate(&self) {
        let _ = self.tx.send(Msg::Deactivate);
    }

    pub fn clear_ink(&self) {
        let _ = self.tx.send(Msg::ClearInk);
    }

    /// AppKit lists our windows; see `ui::refresh_geometry`.
    pub fn set_pass_windows(&self, _windows: Vec<isize>) {}
}

/// A retained IOSurface in BGRA.
struct Surface {
    ptr: Ptr,
    stride: usize,
    height: usize,
}

unsafe impl Send for Surface {}

impl Surface {
    fn new(w: usize, h: usize) -> Option<Surface> {
        autoreleasepool(|_| unsafe {
            let num = |v: i32| -> *mut AnyObject { msg_send![class!(NSNumber), numberWithInt: v] };
            let dict: *mut AnyObject = msg_send![class!(NSMutableDictionary), dictionary];
            let bgra = 0x4247_5241;
            for (key, value) in [(kIOSurfaceWidth, w as i32), (kIOSurfaceHeight, h as i32), (kIOSurfaceBytesPerElement, 4), (kIOSurfacePixelFormat, bgra)] {
                let _: () = msg_send![dict, setObject: num(value), forKey: key as *mut AnyObject];
            }
            let ptr = IOSurfaceCreate(dict as Ptr);
            if ptr.is_null() {
                log::write(format!("IOSurfaceCreate {w}x{h} failed"));
                return None;
            }
            Some(Surface { ptr, stride: IOSurfaceGetBytesPerRow(ptr), height: IOSurfaceGetHeight(ptr) })
        })
    }

    /// Lock it and hand out its pixels.
    fn with_pixels(&mut self, f: impl FnOnce(&mut [u8], usize)) {
        unsafe {
            if IOSurfaceLock(self.ptr, 0, null_mut()) != 0 {
                return;
            }
            let data = std::slice::from_raw_parts_mut(IOSurfaceGetBaseAddress(self.ptr), self.stride * self.height);
            f(data, self.stride);
            IOSurfaceUnlock(self.ptr, 0, null_mut());
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // The layer keeps its own reference for as long as it shows this.
        unsafe { CFRelease(self.ptr) };
    }
}

/// One monitor: `frame` in points, plus its pixel size.
#[derive(Clone, Copy, Default, PartialEq)]
struct Monitor {
    id: u32,
    frame: CGRect,
    w: u32,
    h: u32,
}

impl Monitor {
    fn scale(&self) -> f64 {
        if self.frame.size.width > 0.0 {
            self.w as f64 / self.frame.size.width
        } else {
            1.0
        }
    }

    fn local(&self, p: CGPoint) -> (f32, f32) {
        let s = self.scale();
        (((p.x - self.frame.origin.x) * s) as f32, ((p.y - self.frame.origin.y) * s) as f32)
    }
}

fn cursor_pos() -> CGPoint {
    unsafe {
        let e = CGEventCreate(null_mut());
        if e.is_null() {
            return CGPoint::default();
        }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        p
    }
}

fn monitor_at(p: CGPoint) -> Option<Monitor> {
    unsafe {
        let (mut id, mut count) = (0u32, 0u32);
        if CGGetDisplaysWithPoint(p, 1, &mut id, &mut count) != 0 || count == 0 {
            id = CGMainDisplayID();
        }
        let frame = CGDisplayBounds(id);
        // AppKit's scale is the one the window's backing store really uses.
        let known = ui::geometry().lock().ok().and_then(|g| g.screens.iter().find(|s| s.frame == frame).map(|s| s.scale));
        if let Some(scale) = known {
            let (w, h) = ((frame.size.width * scale).round() as u32, (frame.size.height * scale).round() as u32);
            return Some(Monitor { id, frame, w, h });
        }
        let mode = CGDisplayCopyDisplayMode(id);
        if mode.is_null() {
            return None;
        }
        let (w, h) = (CGDisplayModeGetPixelWidth(mode) as u32, CGDisplayModeGetPixelHeight(mode) as u32);
        CGDisplayModeRelease(mode);
        Some(Monitor { id, frame, w, h })
    }
}

fn frame_interval(id: u32) -> Duration {
    let hz = unsafe {
        let mode = CGDisplayCopyDisplayMode(id);
        if mode.is_null() {
            60.0
        } else {
            let r = CGDisplayModeGetRefreshRate(mode);
            CGDisplayModeRelease(mode);
            r
        }
    };
    let hz = if hz < 1.0 { 60.0 } else { hz.clamp(30.0, 120.0) };
    Duration::from_secs_f64(1.0 / hz)
}

/// The open pop-up menus on screen, anyone's.
fn open_menus() -> Vec<CGRect> {
    let mut out = Vec::new();
    unsafe {
        let list = CGWindowListCopyWindowInfo(LIST_ON_SCREEN | LIST_NO_DESKTOP_ELEMENTS, 0);
        if list.is_null() {
            return out;
        }
        let level = CGWindowLevelForKey(LEVEL_KEY_POPUP_MENU);
        for i in 0..CFArrayGetCount(list) {
            let info = CFArrayGetValueAtIndex(list, i);
            let layer = CFDictionaryGetValue(info, kCGWindowLayer);
            let mut value = 0i32;
            if layer.is_null() || !CFNumberGetValue(layer, CF_NUMBER_INT, &mut value as *mut i32 as *mut _) || value != level {
                continue;
            }
            let bounds = CFDictionaryGetValue(info, kCGWindowBounds);
            let mut rect = CGRect::default();
            if !bounds.is_null() && CGRectMakeWithDictionaryRepresentation(bounds, &mut rect) {
                out.push(rect);
            }
        }
        CFRelease(list);
    }
    out
}

/// Where the Dock is, read off the overlay thread because it can be slow.
mod dock {
    use super::*;

    static READING: AtomicBool = AtomicBool::new(false);

    fn frame() -> &'static Mutex<Option<CGRect>> {
        static F: OnceLock<Mutex<Option<CGRect>>> = OnceLock::new();
        F.get_or_init(|| Mutex::new(None))
    }

    pub fn contains(p: CGPoint) -> bool {
        frame().lock().map(|f| f.is_some_and(|r| r.inset(-8.0, -8.0).contains(p))).unwrap_or(false)
    }

    pub fn refresh() {
        if READING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(|| {
            let fresh = read();
            if let Ok(mut f) = frame().lock() {
                *f = fresh;
            }
            READING.store(false, Ordering::SeqCst);
        });
    }

    fn dock_pid() -> Option<i32> {
        autoreleasepool(|_| unsafe {
            let id: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: c"com.apple.dock".as_ptr()];
            let apps: *mut AnyObject = msg_send![class!(NSRunningApplication), runningApplicationsWithBundleIdentifier: id];
            let first: *mut AnyObject = msg_send![apps, firstObject];
            if first.is_null() {
                return None;
            }
            let pid: i32 = msg_send![first, processIdentifier];
            Some(pid)
        })
    }

    fn cfstring(s: &std::ffi::CStr) -> Ptr {
        unsafe {
            let o: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: s.as_ptr()];
            o as Ptr
        }
    }

    fn read() -> Option<CGRect> {
        let pid = dock_pid()?;
        autoreleasepool(|_| unsafe {
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                return None;
            }
            AXUIElementSetMessagingTimeout(app, 0.25);
            let mut children = null_mut();
            let mut result = None;
            if AXUIElementCopyAttributeValue(app, cfstring(c"AXChildren"), &mut children) == 0 && !children.is_null() {
                if CFArrayGetCount(children) > 0 {
                    let list = CFArrayGetValueAtIndex(children, 0);
                    let (mut pos, mut size) = (null_mut(), null_mut());
                    let mut rect = CGRect::default();
                    AXUIElementCopyAttributeValue(list, cfstring(c"AXPosition"), &mut pos);
                    AXUIElementCopyAttributeValue(list, cfstring(c"AXSize"), &mut size);
                    if !pos.is_null() {
                        AXValueGetValue(pos, AX_VALUE_POINT, &mut rect.origin as *mut CGPoint as *mut _);
                        CFRelease(pos);
                    }
                    if !size.is_null() {
                        AXValueGetValue(size, AX_VALUE_SIZE, &mut rect.size as *mut CGSize as *mut _);
                        CFRelease(size);
                    }
                    if !rect.is_empty() {
                        result = Some(rect);
                    }
                }
                CFRelease(children);
            }
            CFRelease(app);
            result
        })
    }
}

const IDLE: Duration = Duration::from_millis(50);
const STILL_AFTER: Duration = Duration::from_millis(500);

struct Runner {
    rx: Receiver<Msg>,
    shared: Arc<Shared>,
    on_event: Box<dyn Fn(OverlayEvent) + Send>,
    engine: Engine,
    look: Option<Look>,
    sound: Option<&'static [u8]>,
    interact: u64,
    surfaces: [Option<Surface>; 2],
    /// The one being shown; the other is painted next.
    front: usize,
    last_dirty: Option<IRect>,
    monitor: Monitor,
    frame_interval: Duration,
    active: bool,
    tap_ok: bool,
    revealed: bool,
    reveal_until: Instant,
    cursor_hidden: bool,
    catching_mouse: bool,
    last_tick: Instant,
    last_move: Instant,
    last_maintain: Instant,
    last_permission_check: Instant,
    last_menu_scan: Instant,
    last_button: Instant,
    menu_rects: Vec<CGRect>,
    last_pt: CGPoint,
    pressed: bool,
    button_seen_down: bool,
    toolbar_pointer: Option<(f32, f32)>,
}

impl Runner {
    fn new(rx: Receiver<Msg>, shared: Arc<Shared>, on_event: Box<dyn Fn(OverlayEvent) + Send>) -> Self {
        let now = Instant::now();
        Runner {
            rx,
            shared,
            on_event,
            engine: Engine::new(Look::Laser, Settings::default()),
            look: None,
            sound: None,
            interact: 0,
            surfaces: [None, None],
            front: 0,
            last_dirty: None,
            monitor: Monitor::default(),
            frame_interval: Duration::from_micros(16_667),
            active: false,
            tap_ok: false,
            revealed: false,
            reveal_until: now,
            cursor_hidden: false,
            catching_mouse: false,
            last_tick: now,
            last_move: now,
            last_maintain: now,
            last_permission_check: now,
            last_menu_scan: now,
            last_button: now,
            menu_rects: Vec::new(),
            last_pt: CGPoint::default(),
            pressed: false,
            button_seen_down: false,
            toolbar_pointer: None,
        }
    }

    fn run(mut self) {
        loop {
            let msg = if !self.active {
                match self.rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => return,
                }
            } else {
                let now = Instant::now();
                let busy = now.duration_since(self.last_move) < STILL_AFTER || self.engine.is_busy(now);
                self.shared.idle.store(!busy, Ordering::Relaxed);
                let wait = if busy { self.frame_interval } else { IDLE }.saturating_sub(now.duration_since(self.last_tick));
                match self.rx.recv_timeout(wait) {
                    Ok(m) => Some(m),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            autoreleasepool(|_| {
                if let Some(m) = msg {
                    self.handle(m);
                    while let Ok(m) = self.rx.try_recv() {
                        self.handle(m);
                    }
                }
                if self.active {
                    self.tick();
                }
            });
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Configure(cfg) => self.configure(cfg),
            Msg::Activate => self.activate(),
            Msg::Deactivate => self.deactivate(),
            Msg::ClearInk => self.engine.clear_ink(),
            Msg::Tap(e) => self.tap_event(e),
        }
    }

    fn configure(&mut self, cfg: OverlayConfig) {
        let same_look = match (&self.look, &cfg.look) {
            (Some(Look::Laser), Look::Laser) => true,
            (Some(Look::Hand(a)), Look::Hand(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if !same_look {
            self.engine.set_look(cfg.look.clone());
            self.look = Some(cfg.look);
        }
        self.engine.set_settings(cfg.settings);
        self.sound = cfg.sound;
        self.interact = match cfg.interact {
            InteractKey::Alt => FLAG_ALT,
            InteractKey::Ctrl => FLAG_CONTROL,
            InteractKey::Shift => FLAG_SHIFT,
            InteractKey::Off => 0,
        };
        self.shared.interact_flag.store(self.interact, Ordering::Relaxed);
    }

    fn tap_event(&mut self, e: TapEvent) {
        if !self.active {
            return;
        }
        self.last_button = Instant::now();
        match e {
            TapEvent::Moved => self.last_move = Instant::now(),
            TapEvent::Down { left } => {
                let at = self.monitor.local(cursor_pos());
                self.engine.mouse_down(left, at);
                if let Some(wav) = self.sound {
                    crate::audio::play(wav);
                }
                self.pressed = true;
                self.button_seen_down = false;
                self.set_revealed(false);
            }
            TapEvent::Up => self.release(),
        }
    }

    fn release(&mut self) {
        self.pressed = false;
        self.button_seen_down = false;
        self.engine.mouse_up(Instant::now());
    }

    fn activate(&mut self) {
        if self.active {
            return;
        }
        self.active = true;
        self.shared.reset();
        self.shared.active.store(true, Ordering::SeqCst);
        crate::audio::hold(true);
        on_main(|| {
            cursor::enable_background_hiding();
            ui::refresh_geometry();
        });
        self.install_tap();
        if !self.tap_ok {
            log::write("no Accessibility permission yet; clicks will go through");
            on_main(ask_accessibility);
        }
        self.monitor = Monitor::default();
        let now = Instant::now();
        self.last_tick = now;
        self.last_move = now;
        self.last_maintain = now - Duration::from_secs(1);
        self.reveal_until = now + Duration::from_secs(3);
        self.revealed = false;
        self.set_cursor_hidden(true);
        self.tick();
        log::write(format!("overlay on, tap={}", self.tap_ok));
    }

    fn install_tap(&mut self) {
        self.tap_ok = tap::install(&self.shared);
        if self.tap_ok {
            tap::set_enabled(self.active);
        }
    }

    fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.shared.active.store(false, Ordering::SeqCst);
        tap::set_enabled(false);
        crate::audio::hold(false);
        self.pressed = false;
        self.engine.mouse_up(Instant::now());
        self.engine.end_session();
        self.surfaces = [None, None];
        self.last_dirty = None;
        self.monitor = Monitor::default();
        self.catching_mouse = false;
        self.set_cursor_hidden(false);
        on_main(|| {
            ui::overlay_catch_mouse(false);
            ui::hide_all();
        });
        self.revealed = false;
        (self.on_event)(OverlayEvent::Reveal(false));
    }

    fn set_cursor_hidden(&mut self, hidden: bool) {
        if hidden != self.cursor_hidden {
            self.cursor_hidden = hidden;
            on_main(move || if hidden { cursor::hide() } else { cursor::show() });
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;
        let pt = cursor_pos();
        if pt != self.last_pt {
            self.last_pt = pt;
            self.last_move = now;
        }
        self.follow_monitor(pt);
        if now.duration_since(self.last_maintain) > Duration::from_millis(250) {
            self.maintain(now);
        }

        let geo = ui::geometry().lock().map(|g| g.clone()).unwrap_or_default();
        let screen = geo.screens.iter().find(|s| s.frame == self.monitor.frame).copied();
        let menu_bar = screen.map(|s| {
            let height = (s.visible.origin.y - s.frame.origin.y).max(24.0);
            CGRect::new(s.frame.origin.x, s.frame.origin.y, s.frame.size.width, height)
        });

        // Pop-up menus: scan quickly while one is likely.
        let near_bar = menu_bar.is_some_and(|b| b.inset(0.0, -40.0).contains(pt));
        let menu_likely = !self.menu_rects.is_empty() || near_bar || now.duration_since(self.last_button) < Duration::from_secs(2);
        let every = Duration::from_millis(if menu_likely { 50 } else { 250 });
        if now.duration_since(self.last_menu_scan) > every {
            self.last_menu_scan = now;
            self.menu_rects = open_menus();
        }

        let flags = unsafe { CGEventSourceFlagsState(SOURCE_HID) };
        let interacting = (self.interact != 0 && flags & self.interact != 0) || self.shared.interact_press.load(Ordering::Relaxed);
        let over_menu = menu_bar.is_some_and(|b| b.contains(pt)) || self.menu_rects.iter().any(|r| r.contains(pt));
        let over_own = geo.own_windows.iter().any(|r| r.contains(pt));
        let real_cursor = !self.engine.is_drawing() && (interacting || over_menu || over_own);
        self.set_cursor_hidden(!real_cursor);
        self.engine.set_visible(!real_cursor);

        let solid = geo.toolbar.map(|r| {
            let cut = r.size.height * crate::platform::toolbar_solid_top();
            CGRect::new(r.origin.x, r.origin.y + cut, r.size.width, r.size.height - cut)
        });
        self.update_reveal(pt, now, solid);
        self.update_toolbar_pointer(pt, geo.toolbar);

        // Over the Dock, keep winning the fight for the cursor.
        let over_dock = dock::contains(pt);
        let catch = over_dock && self.tap_ok && !real_cursor;
        if catch != self.catching_mouse {
            self.catching_mouse = catch;
            on_main(move || ui::overlay_catch_mouse(catch));
        }
        if !real_cursor {
            let urgent = now.duration_since(self.last_button) < Duration::from_secs(2);
            on_main(move || cursor::reassert(over_dock, urgent));
        }

        let mut pass: Vec<CGRect> = geo.own_windows.clone();
        pass.extend(menu_bar);
        pass.extend(self.menu_rects.iter().copied());
        if self.revealed {
            pass.extend(solid);
        }
        if let Ok(mut p) = self.shared.pass.lock() {
            *p = pass;
        }

        // A release can be lost if the tap was briefly disabled.
        if self.pressed {
            let down = unsafe { CGEventSourceButtonState(SOURCE_HID, 0) };
            if down {
                self.button_seen_down = true;
            } else if self.button_seen_down {
                self.release();
            }
        }

        let local = self.monitor.local(pt);
        if let Some(dirty) = self.engine.frame(now, dt, local) {
            self.present(dirty, now);
        }
    }

    fn follow_monitor(&mut self, pt: CGPoint) {
        let Some(m) = monitor_at(pt) else { return };
        if m == self.monitor && self.surfaces[0].is_some() {
            return;
        }
        if self.pressed {
            self.release();
        }
        self.monitor = m;
        self.surfaces = [Surface::new(m.w as usize, m.h as usize), Surface::new(m.w as usize, m.h as usize)];
        self.front = 0;
        self.last_dirty = None;
        self.engine.set_monitor((m.frame.origin.x as i32, m.frame.origin.y as i32, m.w, m.h));
        self.frame_interval = frame_interval(m.id);
        let frame = m.frame;
        on_main(move || {
            ui::overlay_show(frame);
            ui::refresh_geometry();
        });
        self.reveal_until = self.reveal_until.max(Instant::now() + Duration::from_millis(1500));
        self.last_maintain = Instant::now() - Duration::from_secs(1);
        log::write(format!("overlay on monitor {}x{} ({:?}/frame)", m.w, m.h, self.frame_interval));
    }

    fn maintain(&mut self, now: Instant) {
        self.last_maintain = now;
        let monitor = self.monitor.frame;
        on_main(move || {
            ui::refresh_geometry();
            let geo = ui::geometry().lock().map(|g| g.clone()).unwrap_or_default();
            if let Some(s) = geo.screens.iter().find(|s| s.frame == monitor) {
                ui::place_toolbar(s.visible);
            }
        });
        dock::refresh();
        if !self.tap_ok && now.duration_since(self.last_permission_check) > Duration::from_secs(1) {
            self.last_permission_check = now;
            if unsafe { AXIsProcessTrusted() } {
                self.install_tap();
                if self.tap_ok {
                    log::write("permission granted while running; tap installed");
                }
            }
        }
    }

    fn update_reveal(&mut self, pt: CGPoint, now: Instant, toolbar: Option<CGRect>) {
        let Some(r) = toolbar else { return };
        let zone = CGRect::new(r.origin.x - 80.0, r.origin.y - 100.0, r.size.width + 160.0, r.size.height + 110.0);
        let reveal = !self.engine.is_drawing() && (zone.contains(pt) || now < self.reveal_until);
        self.set_revealed(reveal);
    }

    fn update_toolbar_pointer(&mut self, pt: CGPoint, toolbar: Option<CGRect>) {
        let at = toolbar.filter(|r| self.revealed && r.contains(pt) && !r.is_empty()).map(|r| {
            (((pt.x - r.origin.x) / r.size.width) as f32, ((pt.y - r.origin.y) / r.size.height) as f32)
        });
        if at != self.toolbar_pointer {
            self.toolbar_pointer = at;
            (self.on_event)(OverlayEvent::ToolbarPointer(at));
        }
    }

    fn set_revealed(&mut self, reveal: bool) {
        if reveal != self.revealed {
            self.revealed = reveal;
            (self.on_event)(OverlayEvent::Reveal(reveal));
        }
    }

    /// Paint the back surface and flip to it. It last saw the frame before
    /// the one showing, so it also needs what changed in that one.
    fn present(&mut self, dirty: IRect, now: Instant) {
        let area = match self.last_dirty {
            Some(prev) => dirty.union(prev),
            None => IRect::new(0, 0, self.monitor.w as i32, self.monitor.h as i32),
        };
        self.last_dirty = Some(dirty);
        let back = 1 - self.front;
        let Some(surface) = self.surfaces[back].as_mut() else { return };
        let engine = &self.engine;
        surface.with_pixels(|data, stride| engine.render_bgra(area, now, data, stride));
        let ptr = surface.ptr as usize;
        self.front = back;
        on_main(move || ui::overlay_present(ptr));
    }
}

/// Show the system's Accessibility prompt.
fn ask_accessibility() {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: Ptr) -> bool;
        static kAXTrustedCheckOptionPrompt: Ptr;
    }
    autoreleasepool(|_| unsafe {
        let yes: *mut AnyObject = msg_send![class!(NSNumber), numberWithBool: objc2::runtime::Bool::YES];
        let dict: *mut AnyObject = msg_send![class!(NSDictionary), dictionaryWithObject: yes, forKey: kAXTrustedCheckOptionPrompt as *mut AnyObject];
        AXIsProcessTrustedWithOptions(dict as Ptr);
    });
}
