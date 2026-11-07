//! Drawing on the screen. Started as a port of `Ink.swift`: smooth quadratic
//! strokes with a soft shadow that fade out a few seconds after the button is
//! released.

use std::time::Instant;

use tiny_skia::{Color, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke as SkStroke, Transform};

use crate::geom::{merge, IRect};

const FADE_DURATION: f32 = 0.6;
const MIN_STEP: f32 = 1.5;
const SHADOW_OFFSET: (f32, f32) = (1.0, 1.5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub color: [u8; 3],
    pub width: f32,
    pub shadow: bool,
}

struct Stroke {
    pts: Vec<(f32, f32)>,
    pen: Pen,
    ended: Option<Instant>,
    min: (f32, f32),
    max: (f32, f32),
}

impl Stroke {
    fn pad(&self) -> f32 {
        self.pen.width / 2.0 + 4.0
    }

    fn bounds(&self) -> IRect {
        IRect::around(self.min.0, self.min.1, self.max.0 + SHADOW_OFFSET.0, self.max.1 + SHADOW_OFFSET.1, self.pad())
    }

    fn alpha(&self, now: Instant, fade_seconds: f32) -> f32 {
        let Some(ended) = self.ended else { return 1.0 };
        if fade_seconds <= 0.0 {
            return 1.0;
        }
        let delay = fade_seconds.max(0.5);
        let t = now.saturating_duration_since(ended).as_secs_f32();
        if t < delay {
            return 1.0;
        }
        let u = ((t - delay) / FADE_DURATION).min(1.0);
        1.0 - u * u
    }

    fn path(&self) -> Option<Path> {
        let mut pb = PathBuilder::new();
        let first = *self.pts.first()?;
        pb.move_to(first.0, first.1);
        if self.pts.len() == 1 {
            pb.line_to(first.0 + 0.01, first.1);
        }
        for w in self.pts.windows(2) {
            let (prev, p) = (w[0], w[1]);
            pb.quad_to(prev.0, prev.1, (prev.0 + p.0) / 2.0, (prev.1 + p.1) / 2.0);
        }
        if self.pts.len() > 1 {
            let last = self.pts[self.pts.len() - 1];
            pb.line_to(last.0, last.1);
        }
        pb.finish()
    }

    fn draw(&self, pm: &mut Pixmap, offset: Transform, alpha: f32) {
        let Some(path) = self.path() else { return };
        let stroke = SkStroke { width: self.pen.width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let mut paint = Paint { anti_alias: true, ..Default::default() };
        if self.pen.shadow {
            paint.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.18 * alpha).unwrap_or(Color::TRANSPARENT));
            let shadow = offset.pre_translate(SHADOW_OFFSET.0, SHADOW_OFFSET.1);
            pm.stroke_path(&path, &paint, &stroke, shadow, None);
        }
        let [r, g, b] = self.pen.color;
        paint.set_color_rgba8(r, g, b, (alpha * 255.0).round() as u8);
        pm.stroke_path(&path, &paint, &stroke, offset, None);
    }

}

#[derive(Default)]
pub struct Ink {
    strokes: Vec<Stroke>,
    drawing: bool,
}

impl Ink {
    pub fn is_drawing(&self) -> bool {
        self.drawing
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    pub fn begin(&mut self, p: (f32, f32), pen: Pen) -> IRect {
        let s = Stroke { pts: vec![p], pen, ended: None, min: p, max: p };
        let b = s.bounds();
        self.strokes.push(s);
        self.drawing = true;
        b
    }

    /// Add a point to the current stroke; returns the area that changed.
    pub fn extend(&mut self, p: (f32, f32)) -> Option<IRect> {
        if !self.drawing {
            return None;
        }
        let s = self.strokes.last_mut()?;
        let last = *s.pts.last()?;
        if (p.0 - last.0).hypot(p.1 - last.1) < MIN_STEP {
            return None;
        }
        s.pts.push(p);
        s.min = (s.min.0.min(p.0), s.min.1.min(p.1));
        s.max = (s.max.0.max(p.0), s.max.1.max(p.1));
        // The curve between the last few points is what moved.
        let tail = &s.pts[s.pts.len().saturating_sub(3)..];
        let (mut lo, mut hi) = (tail[0], tail[0]);
        for q in tail {
            lo = (lo.0.min(q.0), lo.1.min(q.1));
            hi = (hi.0.max(q.0), hi.1.max(q.1));
        }
        Some(IRect::around(lo.0, lo.1, hi.0 + SHADOW_OFFSET.0, hi.1 + SHADOW_OFFSET.1, s.pad()))
    }

    pub fn end(&mut self, now: Instant) {
        if !self.drawing {
            return;
        }
        self.drawing = false;
        let Some(s) = self.strokes.last_mut() else { return };
        s.ended = Some(now);
    }

    /// Wipe everything.
    pub fn clear(&mut self) -> Option<IRect> {
        let area = self.strokes.iter().map(Stroke::bounds).reduce(IRect::union);
        self.strokes.clear();
        self.drawing = false;
        area
    }

    pub fn area(&self) -> Option<IRect> {
        self.strokes.iter().map(Stroke::bounds).reduce(IRect::union)
    }

    pub fn is_fading(&self, now: Instant, fade_seconds: f32) -> bool {
        fade_seconds > 0.0 && self.strokes.iter().any(|s| s.ended.is_some() && s.alpha(now, fade_seconds) < 1.0)
    }

    /// Drop finished strokes; returns the area that needs repainting because
    /// a stroke is fading or just disappeared.
    pub fn tick(&mut self, now: Instant, fade_seconds: f32) -> Option<IRect> {
        let mut dirty = None;
        self.strokes.retain(|s| {
            let a = s.alpha(now, fade_seconds);
            if a < 1.0 {
                dirty = merge(dirty, Some(s.bounds()));
            }
            a > 0.0
        });
        dirty
    }

    /// Draw into `pm`, a fresh transparent pixmap covering `clip`.
    pub fn render(&self, pm: &mut Pixmap, offset: Transform, now: Instant, fade_seconds: f32, clip: IRect) {
        for s in &self.strokes {
            if s.bounds().intersect(clip).is_some() {
                s.draw(pm, offset, s.alpha(now, fade_seconds));
            }
        }
    }

    #[cfg(test)]
    fn stroke_count(&self) -> usize {
        self.strokes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const PEN: Pen = Pen { color: [255, 0, 0], width: 4.0, shadow: false };

    fn line(ink: &mut Ink, from: (f32, f32), to: (f32, f32), now: Instant) {
        ink.begin(from, PEN);
        let steps = 20;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            ink.extend((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
        }
        ink.end(now);
    }

    #[test]
    fn strokes_fade_unless_kept() {
        let mut ink = Ink::default();
        let t = Instant::now();
        line(&mut ink, (10.0, 10.0), (100.0, 10.0), t);
        let later = t + Duration::from_secs(10);
        ink.tick(later, 0.0);
        assert_eq!(ink.stroke_count(), 1, "kept");
        ink.tick(later, 3.0);
        assert!(ink.is_empty(), "faded");
    }
}
