//! The overlay's frame loop, minus the OS. A platform layer:
//!
//! 1. calls [`Engine::set_monitor`] with the monitor it's on,
//! 2. forwards button presses with [`Engine::mouse_down`] / [`Engine::mouse_up`],
//! 3. calls [`Engine::frame`] every tick with the cursor in monitor pixels, and
//! 4. when that returns a dirty rect, copies [`Engine::render_bgra`] (or
//!    [`Engine::render`]) into its transparent window.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tiny_skia::{Color, FillRule, GradientStop, Paint, PathBuilder, Pixmap, Point, RadialGradient, SpreadMode, Transform};

use crate::arm::{Arm, HandPose, LaserPose};
use crate::asset::{DisplayHand, HandAsset};
use crate::geom::{merge, Affine, IRect};
use crate::ink::{Ink, Pen};
use crate::raster;

#[derive(Clone)]
pub enum Look {
    Hand(Arc<HandAsset>),
    Laser,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub size: f32,
    pub shadow: bool,
    /// 0 keeps strokes until cleared.
    pub fade_seconds: f32,
    pub ink: [u8; 3],
    pub pen_scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            size: 0.13,
            shadow: true,
            fade_seconds: 3.0,
            ink: crate::catalog::rgb(crate::catalog::PURPLE),
            pen_scale: 1.0,
        }
    }
}

enum Pose {
    Hand(HandPose),
    Laser(LaserPose),
}

/// What the current press is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    /// Nothing yet. It's a tap unless it moves far enough to start something.
    Pressed,
    Freehand,
}

/// Identifies a monitor: where it is and how big.
pub type MonitorKey = (i32, i32, u32, u32);

pub struct Engine {
    w: u32,
    h: u32,
    look: Look,
    display: Option<DisplayHand>,
    settings: Settings,
    arm: Arm,
    monitor: MonitorKey,
    ink: Ink,
    /// Other monitors' drawings, for when the cursor comes back.
    parked: HashMap<MonitorKey, Ink>,
    visible: bool,
    down: Option<(f32, f32)>,
    left_down: bool,
    gesture: Option<Gesture>,
    pose: Option<Pose>,
    last_bounds: Option<IRect>,
    last_key: Option<[i32; 6]>,
    pending: Option<IRect>,
}

const LASER_RED: [u8; 3] = [255, 41, 31];

impl Engine {
    pub fn new(look: Look, settings: Settings) -> Self {
        Self {
            w: 0,
            h: 0,
            look,
            display: None,
            settings,
            arm: Arm::default(),
            monitor: (0, 0, 0, 0),
            ink: Ink::default(),
            parked: HashMap::new(),
            visible: true,
            down: None,
            left_down: false,
            gesture: None,
            pose: None,
            last_bounds: None,
            last_key: None,
            pending: None,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn screen(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// A single monitor of this size. Mostly for tests and tools.
    pub fn set_screen(&mut self, w: u32, h: u32) {
        self.set_monitor((0, 0, w, h));
    }

    /// Move to a monitor. Its drawings come back if the cursor has been there
    /// before; the ones here wait until the cursor returns. Repaints everything.
    pub fn set_monitor(&mut self, key: MonitorKey) {
        let (w, h) = (key.2, key.3);
        if (w, h) != (self.w, self.h) {
            self.display = None;
        }
        if key != self.monitor {
            let incoming = self.parked.remove(&key).unwrap_or_default();
            let outgoing = std::mem::replace(&mut self.ink, incoming);
            if self.monitor.2 > 0 {
                self.parked.insert(self.monitor, outgoing);
            }
            self.monitor = key;
        }
        self.w = w;
        self.h = h;
        self.arm.reset();
        self.down = None;
        self.gesture = None;
        self.pose = None;
        self.last_bounds = None;
        self.last_key = None;
        self.pending = Some(self.full());
    }

    pub fn set_look(&mut self, look: Look) {
        self.look = look;
        self.display = None;
        self.invalidate_pose();
    }

    pub fn set_settings(&mut self, s: Settings) {
        if s.size != self.settings.size {
            self.display = None;
        }
        if s.fade_seconds != self.settings.fade_seconds {
            self.pending = Some(self.full());
        }
        self.settings = s;
        self.invalidate_pose();
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible != self.visible {
            self.visible = visible;
            self.invalidate_pose();
        }
    }

    /// A button went down with the cursor at `at` (monitor pixels).
    pub fn mouse_down(&mut self, left: bool, at: (f32, f32)) {
        self.down = Some(at);
        self.left_down = left;
        self.gesture = Some(Gesture::Pressed);
        self.arm.press();
        self.arm.held = true;
    }

    pub fn mouse_up(&mut self, now: Instant) {
        self.arm.held = false;
        self.down = None;
        self.gesture = None;
        self.ink.end(now);
    }

    /// Clear the drawing.
    pub fn clear_ink(&mut self) {
        self.pending = merge(self.pending, self.ink.clear());
    }

    /// The pointer turned off: drawings go (they annotate whatever was on screen).
    pub fn end_session(&mut self) {
        self.pending = merge(self.pending, self.ink.clear());
        for ink in self.parked.values_mut() {
            ink.clear();
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.ink.is_drawing()
    }

    pub fn has_ink(&self) -> bool {
        !self.ink.is_empty()
    }

    /// True while something is still animating even if the mouse is still.
    pub fn is_busy(&self, now: Instant) -> bool {
        self.arm.is_settling() || self.gesture.is_some() || self.ink.is_fading(now, self.settings.fade_seconds)
    }

    fn full(&self) -> IRect {
        IRect::new(0, 0, self.w as i32, self.h as i32)
    }

    fn invalidate_pose(&mut self) {
        self.pending = merge(self.pending, self.last_bounds.take());
        self.last_key = None;
        self.pose = None;
    }

    fn ink_color(&self) -> [u8; 3] {
        match self.look {
            Look::Laser => LASER_RED,
            Look::Hand(_) => self.settings.ink,
        }
    }

    fn pen(&self) -> Pen {
        let base = (self.h as f32 * 0.0055).max(3.0) * self.settings.pen_scale;
        Pen { color: self.ink_color(), width: base, shadow: true }
    }

    /// Advance whatever the current press is doing.
    fn advance_gesture(&mut self, cursor: (f32, f32)) {
        let Some(gesture) = self.gesture else { return };
        let dirty = match gesture {
            Gesture::Pressed => {
                let Some(start) = self.down else { return };
                if !self.left_down || (cursor.0 - start.0).hypot(cursor.1 - start.1) < 4.0 {
                    return;
                }
                self.start_drawing(start, cursor)
            }
            Gesture::Freehand => self.ink.extend(cursor),
        };
        self.pending = merge(self.pending, dirty);
    }

    /// The press moved far enough: it's a drag, so start the current tool.
    fn start_drawing(&mut self, start: (f32, f32), cursor: (f32, f32)) -> Option<IRect> {
        let pen = self.pen();
        self.gesture = Some(Gesture::Freehand);
        let begun = self.ink.begin(start, pen);
        merge(Some(begun), self.ink.extend(cursor))
    }

    fn ensure_display(&mut self) {
        let Look::Hand(a) = &self.look else { return };
        if self.display.is_some() || self.h == 0 {
            return;
        }
        // Pick a copy close to the drawn size so drawing is ~1:1 and sharp.
        let full = a.image.height;
        let rest = self.settings.size * self.h as f32 / (a.wrist - a.hand_top).max(1e-3);
        let step = 64.0;
        let needed = ((rest / step).ceil() * step) as u32;
        let height = if needed >= full * 9 / 10 { full } else { needed.max(64) };
        let blur = 0.021 * self.h as f32 * height as f32 / rest.max(1.0);
        self.display = Some(a.display(height, blur));
    }

    /// Advance one tick. Returns the area of the screen that must be redrawn.
    pub fn frame(&mut self, now: Instant, dt: f32, cursor: (f32, f32)) -> Option<IRect> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        self.advance_gesture(cursor);
        let fade = self.settings.fade_seconds;
        let dirty = self.ink.tick(now, fade);
        self.pending = merge(self.pending, dirty);

        self.arm.step(dt, cursor.0, self.w as f32);

        if self.visible {
            self.ensure_display();
            let screen = (self.w as f32, self.h as f32);
            let pose = match (&self.look, &self.display) {
                (Look::Hand(a), Some(d)) => Some(Pose::Hand(self.arm.hand_pose(
                    cursor,
                    screen,
                    a,
                    self.settings.size,
                    self.settings.shadow,
                    d.pixmap.width() as f32,
                    d.pixmap.height() as f32,
                ))),
                (Look::Laser, _) => Some(Pose::Laser(self.arm.laser_pose(cursor, screen.1, self.settings.size))),
                _ => None,
            };
            if let Some(pose) = pose {
                let key = match &pose {
                    Pose::Hand(p) => p.key,
                    Pose::Laser(p) => p.key,
                };
                if Some(key) != self.last_key {
                    let bounds = self.bounds(&pose);
                    self.pending = merge(merge(self.pending, self.last_bounds), Some(bounds));
                    self.last_bounds = Some(bounds);
                    self.last_key = Some(key);
                    self.pose = Some(pose);
                }
            }
        }

        self.pending.take().and_then(|r| r.intersect(self.full()))
    }

    fn bounds(&self, pose: &Pose) -> IRect {
        match pose {
            Pose::Laser(p) => {
                let g = p.r * (3.2 + p.press) + 2.0;
                IRect::around(p.x - g, p.y - g, p.x + g, p.y + g, 1.0)
            }
            Pose::Hand(p) => {
                let Some(d) = self.display.as_ref() else { return IRect::new(0, 0, 0, 0) };
                let blur = d.blur;
                let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
                let mut add = |t: &Affine, r: &[f32; 4], off: (f32, f32)| {
                    for (x, y) in [(r[0], r[1]), (r[2], r[1]), (r[0], r[3]), (r[2], r[3])] {
                        let (sx, sy) = t.apply(x, y);
                        let (sx, sy) = (sx + off.0, sy + off.1);
                        lo = (lo.0.min(sx), lo.1.min(sy));
                        hi = (hi.0.max(sx), hi.1.max(sy));
                    }
                };
                add(&p.hand, &p.hand_src, (0.0, 0.0));
                add(&p.rod, &p.rod_src, (0.0, 0.0));
                if p.shadow_alpha > 0.0 {
                    add(&p.hand, &hand_shadow_src(p, d), p.shadow_offset);
                    add(&p.rod, &rod_shadow_src(p, d), p.shadow_offset);
                }
                IRect::around(lo.0, lo.1, hi.0, hi.1, blur * 1.5 + 3.0)
            }
        }
    }

    /// Render `dirty` (screen pixels) into a premultiplied RGBA pixmap of the
    /// same size.
    pub fn render(&self, dirty: IRect, now: Instant) -> Option<Pixmap> {
        let mut pm = Pixmap::new(dirty.width().max(1) as u32, dirty.height().max(1) as u32)?;
        let offset = Transform::from_translate(-dirty.x0 as f32, -dirty.y0 as f32);
        self.ink.render(&mut pm, offset, now, self.settings.fade_seconds, dirty);
        if self.visible {
            match (&self.pose, &self.display) {
                (Some(Pose::Hand(p)), Some(d)) => draw_hand(&mut pm, p, d, (dirty.x0 as f32, dirty.y0 as f32)),
                (Some(Pose::Laser(p)), _) => draw_laser(&mut pm, p, offset),
                _ => {}
            }
        }
        Some(pm)
    }

    /// Render `dirty` straight into a top-down BGRA (premultiplied) buffer
    /// covering the whole screen, as Windows layered windows expect.
    pub fn render_bgra(&self, dirty: IRect, now: Instant, dst: &mut [u8], stride: usize) {
        let Some(pm) = self.render(dirty, now) else { return };
        let row = dirty.width() as usize * 4;
        for (y, src) in pm.data().chunks_exact(row).enumerate() {
            let start = (dirty.y0 as usize + y) * stride + dirty.x0 as usize * 4;
            let Some(out) = dst.get_mut(start..start + row) else { break };
            for (o, s) in out.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                o[0] = s[2];
                o[1] = s[1];
                o[2] = s[0];
                o[3] = s[3];
            }
        }
    }
}

/// A slice grown by the shadow blur so the shadow isn't cut off.
fn grow(src: [f32; 4], d: &DisplayHand, vertical: bool) -> [f32; 4] {
    let m = d.blur * 1.5;
    let (w, h) = (d.pixmap.width() as f32, d.pixmap.height() as f32);
    let [x0, y0, x1, y1] = src;
    let (y0, y1) = if vertical { ((y0 - m).max(0.0), (y1 + m).min(h)) } else { (y0, y1) };
    [(x0 - m).max(0.0), y0, (x1 + m).min(w), y1]
}

fn hand_shadow_src(p: &HandPose, d: &DisplayHand) -> [f32; 4] {
    grow(p.hand_src, d, true)
}

fn rod_shadow_src(p: &HandPose, d: &DisplayHand) -> [f32; 4] {
    [d.rod.shadow_x0, p.rod_src[1], d.rod.shadow_x1, p.rod_src[3]]
}

fn draw_hand(pm: &mut Pixmap, p: &HandPose, d: &DisplayHand, origin: (f32, f32)) {
    let (width, height) = (pm.width() as usize, pm.height() as usize);
    let mut t = raster::Target { data: pm.data_mut(), width, height, origin };
    if p.shadow_alpha > 0.0 {
        let shift = Affine::translate(p.shadow_offset.0, p.shadow_offset.1);
        raster::rod_shadow(&mut t, &d.rod, p.rod.then(shift), rod_shadow_src(p, d), p.shadow_alpha);
        raster::shadow_image(&mut t, &d.shadow_hand, p.hand.then(shift), hand_shadow_src(p, d), p.shadow_alpha);
    }
    raster::rod(&mut t, &d.rod, p.rod, [d.rod.x0, p.rod_src[1], d.rod.x1, p.rod_src[3]]);
    raster::image(&mut t, &d.pixmap, p.hand, p.hand_src, 1.0);
}

fn circle(pm: &mut Pixmap, x: f32, y: f32, r: f32, paint: &Paint, offset: Transform) {
    if let Some(path) = PathBuilder::from_circle(x, y, r) {
        pm.fill_path(&path, paint, FillRule::Winding, offset, None);
    }
}

fn draw_laser(pm: &mut Pixmap, p: &LaserPose, offset: Transform) {
    let [r, g, b] = LASER_RED;
    let glow = p.r * (3.2 + p.press);
    let stops = vec![
        GradientStop::new(0.0, Color::from_rgba8(r, g, b, 150)),
        GradientStop::new(0.3, Color::from_rgba8(r, g, b, 70)),
        GradientStop::new(1.0, Color::from_rgba8(r, g, b, 0)),
    ];
    let center = Point::from_xy(p.x, p.y);
    if let Some(shader) = RadialGradient::new(center, center, glow, stops, SpreadMode::Pad, Transform::identity()) {
        let paint = Paint { shader, anti_alias: true, ..Default::default() };
        circle(pm, p.x, p.y, glow, &paint, offset);
    }
    let mut paint = Paint { anti_alias: true, ..Default::default() };
    paint.set_color_rgba8(r, g, b, 255);
    circle(pm, p.x, p.y, p.r, &paint, offset);
    paint.set_color_rgba8(255, 219, 209, 242);
    circle(pm, p.x, p.y, p.r * 0.45, &paint, offset);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn lavender() -> Arc<HandAsset> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/hand_lavender.png");
        Arc::new(HandAsset::from_png(&std::fs::read(path).expect("asset")).expect("measure"))
    }

    #[test]
    fn measures_builtin_hand() {
        let a = lavender();
        assert!(a.tip < a.wrist && a.wrist < a.rod_end, "tip {} wrist {} rod_end {}", a.tip, a.wrist, a.rod_end);
        assert!(a.rod_x > 0.2 && a.rod_x < 0.8);
    }

    #[test]
    fn fingertip_lands_on_cursor() {
        let a = lavender();
        let mut arm = Arm::default();
        let screen = (1920.0, 1080.0);
        for cursor in [(300.0, 200.0), (1500.0, 900.0), (960.0, 540.0), (50.0, 1000.0)] {
            arm.step(1.0, cursor.0, screen.0);
            let (dw, dh) = (a.image.width as f32, a.image.height as f32);
            let p = arm.hand_pose(cursor, screen, &a, 0.13, true, dw, dh);
            let (tx, ty) = p.hand.apply(a.tip_x * dw, a.tip * dh);
            assert!((tx - cursor.0).abs() < 1.5 && (ty - cursor.1).abs() < 1.5, "tip at {tx},{ty} for cursor {cursor:?}");
        }
    }

    #[test]
    fn renders_hand_and_ink() {
        let mut e = Engine::new(Look::Hand(lavender()), Settings::default());
        e.set_screen(1280, 800);
        let t0 = Instant::now();
        let mut cur = (400.0, 300.0);
        let first = e.frame(t0, 1.0, cur).expect("first frame paints");
        assert_eq!(first, IRect::new(0, 0, 1280, 800));
        // Draw a stroke.
        e.mouse_down(true, cur);
        for i in 1..30 {
            cur = (400.0 + i as f32 * 8.0, 300.0 + (i as f32 * 0.3).sin() * 40.0);
            e.frame(t0 + Duration::from_millis(i * 8), 1.0 / 120.0, cur);
        }
        e.mouse_up(t0 + Duration::from_millis(300));
        let pm = e.render(IRect::new(0, 0, 1280, 800), t0 + Duration::from_millis(300)).unwrap();
        let alpha_at = |x: u32, y: u32| pm.pixel(x, y).unwrap().alpha();
        // Ink along the stroke, nothing in the far top-left corner.
        assert!(alpha_at(480, 300 + ((10.0f32 * 0.3).sin() * 40.0) as u32) > 0 || alpha_at(480, 306) > 0);
        assert_eq!(alpha_at(5, 5), 0);
        // The hand is drawn just below the fingertip.
        assert!(alpha_at(cur.0 as u32, cur.1 as u32 + 6) > 100, "hand below the tip");

        if let Ok(dir) = std::env::var("TINYHAND_RENDER_DIR") {
            pm.save_png(format!("{dir}/frame.png")).unwrap();
        }

        // Ink fades away after the fade delay.
        let later = t0 + Duration::from_secs(5);
        assert!(e.frame(later, 1.0 / 120.0, cur).is_some());
        assert!(!e.has_ink());
    }

    fn engine() -> Engine {
        let settings = Settings { fade_seconds: 0.0, ..Settings::default() };
        let mut e = Engine::new(Look::Laser, settings);
        e.set_screen(800, 600);
        e
    }

    /// Press at `from`, drag to `to` in steps, release.
    fn drag(e: &mut Engine, from: (f32, f32), to: (f32, f32), t: Instant) {
        e.mouse_down(true, from);
        for i in 1..=20 {
            let k = i as f32 / 20.0;
            e.frame(t, 1.0 / 120.0, (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k));
        }
        e.mouse_up(t);
        e.frame(t, 1.0 / 120.0, to);
    }

    #[test]
    fn a_tap_is_a_tap_and_a_drag_draws() {
        let t = Instant::now();
        let mut e = engine();
        e.mouse_down(true, (100.0, 100.0));
        e.frame(t, 0.01, (101.0, 100.0));
        e.mouse_up(t);
        assert!(!e.has_ink());
        drag(&mut e, (100.0, 100.0), (300.0, 100.0), t);
        assert!(e.has_ink());
    }

    #[test]
    fn right_button_does_not_draw() {
        let t = Instant::now();
        let mut e = engine();
        e.mouse_down(false, (100.0, 100.0));
        e.frame(t, 0.01, (300.0, 100.0));
        e.mouse_up(t);
        assert!(!e.has_ink());
    }

    #[test]
    fn drawings_wait_on_their_monitor() {
        let t = Instant::now();
        let mut e = engine();
        e.set_monitor((0, 0, 800, 600));
        drag(&mut e, (100.0, 100.0), (300.0, 100.0), t);
        e.set_monitor((800, 0, 800, 600));
        assert!(!e.has_ink());
        e.set_monitor((0, 0, 800, 600));
        assert!(e.has_ink());
    }

    #[test]
    fn idle_frames_paint_nothing() {
        let mut e = Engine::new(Look::Laser, Settings::default());
        e.set_screen(800, 600);
        let t = Instant::now();
        e.frame(t, 1.0, (100.0, 100.0));
        for _ in 0..400 {
            e.frame(t, 1.0 / 120.0, (100.0, 100.0));
        }
        assert_eq!(e.frame(t, 1.0 / 120.0, (100.0, 100.0)), None);
        assert!(!e.is_busy(t));
    }
}
