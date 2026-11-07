//! The overlay: a borderless, click-through, topmost layered window covering
//! the monitor the cursor is on. Equivalent of `OverlayController.swift`.
//!
//! Everything runs on one thread that owns the window. It sleeps on a
//! channel, so a click (from the hook) or a command wakes it instantly; while
//! the mouse moves or something animates it ticks at the monitor's refresh
//! rate, and otherwise at 20 Hz.
//!
//! Each tick the shared engine returns the rectangle that changed; only that
//! part is rendered and handed to `UpdateLayeredWindowIndirect`.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tinyhand_core::prefs::InteractKey;
use tinyhand_core::{Engine, Look, Settings};
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors, EnumDisplaySettingsW, GdiFlush, GetDC, GetMonitorInfoW,
    MonitorFromPoint, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DEVMODEW,
    DIB_RGB_COLORS, ENUM_CURRENT_SETTINGS, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetAncestor, GetCursorPos, GetWindow, WindowFromPoint, GA_ROOT, GW_HWNDNEXT, GetWindowRect, IsWindow, PeekMessageW,
    RegisterClassExW, SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindowIndirect, HTTRANSPARENT, HWND_TOPMOST, MSG, PM_REMOVE,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, UPDATELAYEREDWINDOWINFO, WM_NCHITTEST,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use super::hook::{contains, Hook, HookEvent, Shared};
use super::cursor;
use crate::log;
use crate::platform::{OverlayConfig, OverlayEvent};

pub enum Msg {
    Configure(OverlayConfig),
    Activate(Option<isize>),
    Deactivate,
    ClearInk,
    PassWindows(Vec<isize>),
    Hook(HookEvent),
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

    pub fn activate(&self, toolbar: Option<isize>) {
        let _ = self.tx.send(Msg::Activate(toolbar));
    }

    pub fn deactivate(&self) {
        let _ = self.tx.send(Msg::Deactivate);
    }

    pub fn clear_ink(&self) {
        let _ = self.tx.send(Msg::ClearInk);
    }

    pub fn set_pass_windows(&self, windows: Vec<isize>) {
        let _ = self.tx.send(Msg::PassWindows(windows));
    }
}

/// A top-down 32-bit DIB selected into a memory DC.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Surface {
    fn new(width: i32, height: i32) -> Option<Surface> {
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) if !bits.is_null() => b,
                _ => {
                    let _ = DeleteDC(dc);
                    log::write(format!("CreateDIBSection {width}x{height} failed"));
                    return None;
                }
            };
            let old = SelectObject(dc, bitmap.into());
            Some(Surface { dc, bitmap, old, bits: bits.cast(), width, height })
        }
    }

    fn pixels(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.width * self.height * 4) as usize) }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

struct Runner {
    rx: Receiver<Msg>,
    shared: Arc<Shared>,
    on_event: Box<dyn Fn(OverlayEvent) + Send>,
    engine: Engine,
    look: Option<Look>,
    sound: Option<&'static [u8]>,
    interact: InteractKey,
    hook: Option<Hook>,
    hwnd: Option<HWND>,
    surface: Option<Surface>,
    monitor: RECT,
    work: RECT,
    frame_interval: Duration,
    toolbar: Option<HWND>,
    toolbar_rect: Option<RECT>,
    pass_windows: Vec<HWND>,
    taskbars: Vec<RECT>,
    active: bool,
    revealed: bool,
    reveal_until: Instant,
    cursor_hidden: bool,
    cursor_hidden_was_real: bool,
    last_tick: Instant,
    last_move: Instant,
    last_maintain: Instant,
    last_raise: Instant,
    last_pt: POINT,
    toolbar_pointer: Option<(f32, f32)>,
}

const IDLE: Duration = Duration::from_millis(50);
const STILL_AFTER: Duration = Duration::from_millis(500);

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
            interact: InteractKey::Alt,
            hook: None,
            hwnd: None,
            surface: None,
            monitor: RECT::default(),
            work: RECT::default(),
            frame_interval: Duration::from_micros(16_667),
            toolbar: None,
            toolbar_rect: None,
            pass_windows: Vec::new(),
            taskbars: Vec::new(),
            active: false,
            revealed: false,
            reveal_until: now,
            cursor_hidden: false,
            cursor_hidden_was_real: false,
            last_tick: now,
            last_move: now,
            last_maintain: now,
            last_raise: now,
            last_pt: POINT::default(),
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
            if let Some(m) = msg {
                self.handle(m);
                while let Ok(m) = self.rx.try_recv() {
                    self.handle(m);
                }
            }
            self.pump();
            if self.active {
                self.tick();
            }
        }
    }

    fn pump(&self) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Configure(cfg) => self.configure(cfg),
            Msg::Activate(toolbar) => self.activate(toolbar),
            Msg::Deactivate => self.deactivate(),
            Msg::ClearInk => self.engine.clear_ink(),
            Msg::PassWindows(list) => {
                self.pass_windows = list.into_iter().map(|h| HWND(h as _)).collect();
                self.last_maintain = Instant::now() - Duration::from_secs(1);
            }
            Msg::Hook(e) => self.hook_event(e),
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
        self.interact = cfg.interact;
        let vk = match cfg.interact {
            InteractKey::Alt => 0x12,
            InteractKey::Ctrl => 0x11,
            InteractKey::Shift => 0x10,
            InteractKey::Off => 0,
        };
        self.shared.interact_vk.store(vk, Ordering::Relaxed);
    }

    fn hook_event(&mut self, e: HookEvent) {
        if !self.active {
            return;
        }
        match e {
            HookEvent::Moved => self.last_move = Instant::now(),
            HookEvent::Down { left } => {
                let pt = cursor_pos();
                let at = ((pt.x - self.monitor.left) as f32, (pt.y - self.monitor.top) as f32);
                self.engine.mouse_down(left, at);
                if let Some(wav) = self.sound {
                    crate::audio::play(wav);
                }
                self.set_revealed(false);
                log::write(format!("press at {},{} left={left}", at.0 as i32, at.1 as i32));
            }
            HookEvent::Up => {
                log::write(format!("release, drew={}", self.engine.is_drawing()));
                self.engine.mouse_up(Instant::now());
            }
            HookEvent::InteractDown | HookEvent::InteractUp => {}
        }
    }

    fn ensure_window(&mut self) -> Option<HWND> {
        if let Some(h) = self.hwnd {
            return Some(h);
        }
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let class = w!("TinyHandOverlay");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("TinyHand"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            );
            match hwnd {
                Ok(h) => {
                    self.hwnd = Some(h);
                    Some(h)
                }
                Err(e) => {
                    log::write(format!("CreateWindowExW failed: {e}"));
                    None
                }
            }
        }
    }

    fn activate(&mut self, toolbar: Option<isize>) {
        if self.active {
            return;
        }
        let Some(hwnd) = self.ensure_window() else { return };
        self.active = true;
        unsafe {
            // Let the channel wait wake us at frame rate, not every 15.6 ms.
            timeBeginPeriod(1);
        }
        self.shared.reset();
        self.shared.active.store(true, Ordering::SeqCst);
        // Wake the audio device now, so the first tap isn't swallowed.
        crate::audio::hold(true);
        self.hook = Hook::install(&self.shared);
        if self.hook.is_none() {
            log::write("mouse hook not installed; clicks will go through");
        }
        self.toolbar = toolbar.map(|h| HWND(h as _));
        self.toolbar_rect = None;
        self.monitor = RECT::default();
        let now = Instant::now();
        self.last_tick = now;
        self.last_move = now;
        self.last_maintain = now - Duration::from_secs(1);
        self.reveal_until = now + Duration::from_secs(3);
        self.revealed = false;
        self.set_cursor_hidden(true);
        unsafe {
            if let Some(t) = self.toolbar {
                let _ = ShowWindow(t, SW_SHOWNOACTIVATE);
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        self.keep_order();
        self.tick();
        log::write(format!("overlay on, hook={}", self.hook.is_some()));
    }

    fn deactivate(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.shared.active.store(false, Ordering::SeqCst);
        self.hook = None;
        crate::audio::hold(false);
        self.engine.mouse_up(Instant::now());
        self.engine.end_session();
        self.surface = None;
        self.monitor = RECT::default();
        self.set_cursor_hidden(false);
        unsafe {
            timeEndPeriod(1);
            if let Some(h) = self.hwnd {
                let _ = ShowWindow(h, SW_HIDE);
            }
            if let Some(t) = self.toolbar {
                let _ = ShowWindow(t, SW_HIDE);
            }
        }
        self.revealed = false;
    }

    fn set_cursor_hidden(&mut self, hidden: bool) {
        if hidden != self.cursor_hidden {
            self.cursor_hidden = hidden;
            if hidden {
                cursor::hide();
            } else {
                cursor::show();
            }
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
        self.keep_order();

        let interacting = self.shared.interacting() || self.shared.interact_held.load(Ordering::Relaxed);
        // Settings counts only where it's actually the window under the
        // cursor, not wherever its rectangle is (it may be covered).
        let over_window = self.over_pass_window(pt);
        self.shared.over_window.store(over_window, Ordering::Relaxed);
        let over_other = over_window || self.taskbars.iter().any(|r| contains(r, pt));
        let real_cursor = !self.engine.is_drawing() && (interacting || over_other);
        if real_cursor != self.cursor_hidden_was_real {
            self.cursor_hidden_was_real = real_cursor;
            let why = if interacting { "interact key" } else if over_window { "settings window" } else { "taskbar" };
            log::write(if real_cursor { format!("real cursor ({why}) at {},{}", pt.x, pt.y) } else { format!("hand back at {},{}", pt.x, pt.y) });
        }
        self.set_cursor_hidden(!real_cursor);
        self.engine.set_visible(!real_cursor);
        self.update_reveal(pt, now);
        self.publish_pass_rects();
        self.update_toolbar_pointer(pt);

        let local = ((pt.x - self.monitor.left) as f32, (pt.y - self.monitor.top) as f32);
        if let Some(dirty) = self.engine.frame(now, dt, local) {
            self.present(dirty, now);
        }
    }

    /// Move the overlay to whichever monitor the cursor is on.
    fn follow_monitor(&mut self, pt: POINT) {
        unsafe {
            let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if !GetMonitorInfoW(hmon, &mut info.monitorInfo as *mut MONITORINFO).as_bool() {
                return;
            }
            let m = info.monitorInfo.rcMonitor;
            self.work = info.monitorInfo.rcWork;
            if m == self.monitor && self.surface.is_some() {
                return;
            }
            self.monitor = m;
            let (w, h) = (m.right - m.left, m.bottom - m.top);
            self.surface = None;
            self.surface = Surface::new(w, h);
            self.engine.set_monitor((m.left, m.top, w as u32, h as u32));
            self.frame_interval = refresh_interval(&info.szDevice);
            if let Some(hwnd) = self.hwnd {
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), m.left, m.top, w, h, SWP_NOACTIVATE);
            }
            self.toolbar_rect = None;
            self.last_maintain = Instant::now() - Duration::from_secs(1);
            log::write(format!("overlay on monitor {w}x{h} at {},{} ({:?}/frame)", m.left, m.top, self.frame_interval));
        }
    }

    /// Slow-changing things, a few times a second: where the taskbars and
    /// pass-through windows are, toolbar placement, staying on top.
    fn maintain(&mut self, now: Instant) {
        self.last_maintain = now;
        self.taskbars = taskbar_rects();
        self.pass_windows.retain(|h| unsafe { IsWindow(Some(*h)).as_bool() });
        self.place_toolbar();
        if now.duration_since(self.last_raise) > Duration::from_secs(1) {
            // Other always-on-top apps can cover us; climb back to the top.
            self.last_raise = now;
            if let Some(h) = self.hwnd {
                unsafe {
                    let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
        self.publish_pass_rects();
    }

    /// The hand must draw over the toolbar. Both are topmost, and Windows
    /// raises the toolbar when it's shown, resized or clicked, so check every
    /// tick and tuck it back directly under the overlay when needed.
    fn keep_order(&self) {
        let (Some(overlay), Some(toolbar)) = (self.hwnd, self.toolbar) else { return };
        if !is_above(overlay, toolbar) {
            unsafe {
                let _ = SetWindowPos(toolbar, Some(overlay), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    /// Keep the toolbar centred just above the taskbar on the current monitor.
    fn place_toolbar(&mut self) {
        let Some(t) = self.toolbar else { return };
        unsafe {
            let mut r = RECT::default();
            if GetWindowRect(t, &mut r).is_err() {
                return;
            }
            let (tw, th) = (r.right - r.left, r.bottom - r.top);
            let margin = (16 * GetDpiForWindow(t).max(96) / 96) as i32;
            let x = self.work.left + (self.work.right - self.work.left - tw) / 2;
            let y = self.work.bottom - th - margin;
            if r.left != x || r.top != y {
                // Move only; z-order is keep_order's job.
                let _ = SetWindowPos(t, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
            }
            self.toolbar_rect = Some(RECT { left: x, top: y, right: x + tw, bottom: y + th });
        }
    }

    /// The part of the toolbar window that's showing something.
    fn toolbar_solid(&self) -> Option<RECT> {
        let r = self.toolbar_rect?;
        let top = r.top + ((r.bottom - r.top) as f64 * crate::platform::toolbar_solid_top()) as i32;
        Some(RECT { top, ..r })
    }

    fn update_toolbar_pointer(&mut self, pt: POINT) {
        let at = self.toolbar_rect.filter(|r| self.revealed && contains(r, pt) && r.right > r.left && r.bottom > r.top).map(|r| {
            ((pt.x - r.left) as f32 / (r.right - r.left) as f32, (pt.y - r.top) as f32 / (r.bottom - r.top) as f32)
        });
        if at != self.toolbar_pointer {
            self.toolbar_pointer = at;
            (self.on_event)(OverlayEvent::ToolbarPointer(at));
        }
    }

    fn publish_pass_rects(&self) {
        let mut rects = self.taskbars.clone();
        if self.revealed {
            rects.extend(self.toolbar_solid());
        }
        if let Ok(mut pass) = self.shared.pass.lock() {
            *pass = rects;
        }
    }

    /// Is the top-level window under the cursor one of ours (Settings)?
    /// The overlay itself is layered + transparent, so hit-testing skips it.
    fn over_pass_window(&self, pt: POINT) -> bool {
        if self.pass_windows.is_empty() {
            return false;
        }
        unsafe {
            let hit = WindowFromPoint(pt);
            if hit.is_invalid() {
                return false;
            }
            let root = GetAncestor(hit, GA_ROOT);
            self.pass_windows.contains(&root)
        }
    }

    /// The toolbar fades out unless the cursor is near it (port of the
    /// macOS reveal zone: a bit wider than the bar and 90pt above it).
    fn update_reveal(&mut self, pt: POINT, now: Instant) {
        let Some(r) = self.toolbar_solid() else { return };
        let scale = self.toolbar.map(|t| unsafe { GetDpiForWindow(t) }).unwrap_or(96).max(96) as i32;
        let s = |v: i32| v * scale / 96;
        let zone = RECT { left: r.left - s(80), top: r.top - s(100), right: r.right + s(80), bottom: r.bottom + s(10) };
        let near = contains(&zone, pt);
        let reveal = !self.engine.is_drawing() && (near || now < self.reveal_until);
        self.set_revealed(reveal);
    }

    fn set_revealed(&mut self, reveal: bool) {
        if reveal != self.revealed {
            self.revealed = reveal;
            (self.on_event)(OverlayEvent::Reveal(reveal));
            self.publish_pass_rects();
        }
    }

    fn present(&mut self, dirty: tinyhand_core::IRect, now: Instant) {
        let (Some(hwnd), Some(surface)) = (self.hwnd, self.surface.as_mut()) else { return };
        let stride = surface.width as usize * 4;
        unsafe {
            let _ = GdiFlush();
        }
        self.engine.render_bgra(dirty, now, surface.pixels(), stride);
        let dst = POINT { x: self.monitor.left, y: self.monitor.top };
        let size = SIZE { cx: surface.width, cy: surface.height };
        let src = POINT::default();
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let rect = RECT { left: dirty.x0, top: dirty.y0, right: dirty.x1, bottom: dirty.y1 };
        let info = UPDATELAYEREDWINDOWINFO {
            cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
            hdcDst: HDC::default(),
            pptDst: &dst,
            psize: &size,
            hdcSrc: surface.dc,
            pptSrc: &src,
            crKey: COLORREF(0),
            pblend: &blend,
            dwFlags: ULW_ALPHA,
            prcDirty: &rect,
        };
        unsafe {
            if !UpdateLayeredWindowIndirect(hwnd, &info).as_bool() {
                log::write(format!("UpdateLayeredWindowIndirect failed: {}", windows::core::Error::from_win32()));
            }
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Is `a` higher than `b` in the z-order? Walks down from `a`.
fn is_above(a: HWND, b: HWND) -> bool {
    let mut w = a;
    for _ in 0..512 {
        match unsafe { GetWindow(w, GW_HWNDNEXT) } {
            Ok(next) if !next.is_invalid() => {
                if next == b {
                    return true;
                }
                w = next;
            }
            _ => return false,
        }
    }
    false
}

fn cursor_pos() -> POINT {
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    pt
}

fn refresh_interval(device: &[u16; 32]) -> Duration {
    let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    let hz = unsafe {
        if EnumDisplaySettingsW(PCWSTR(device.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode).as_bool() {
            mode.dmDisplayFrequency
        } else {
            60
        }
    };
    let hz = if hz <= 1 { 60 } else { hz.clamp(30, 240) };
    Duration::from_secs_f64(1.0 / hz as f64)
}

/// The strip each monitor reserves for a taskbar (monitor minus work area).
fn taskbar_rects() -> Vec<RECT> {
    unsafe extern "system" fn each(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let out = &mut *(data.0 as *mut Vec<RECT>);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(hmon, &mut info).as_bool() {
            let (m, w) = (info.rcMonitor, info.rcWork);
            let strips = [
                RECT { left: m.left, top: w.bottom, right: m.right, bottom: m.bottom },
                RECT { left: m.left, top: m.top, right: m.right, bottom: w.top },
                RECT { left: m.left, top: m.top, right: w.left, bottom: m.bottom },
                RECT { left: w.right, top: m.top, right: m.right, bottom: m.bottom },
            ];
            out.extend(strips.into_iter().filter(|r| r.right > r.left && r.bottom > r.top));
        }
        BOOL(1)
    }
    let mut out: Vec<RECT> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut out as *mut _ as isize));
    }
    out
}
