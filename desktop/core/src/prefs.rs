use serde::{Deserialize, Serialize};

use crate::catalog;
use crate::tools;

pub const POINTER_SIZE_MIN: f32 = 0.08;
pub const POINTER_SIZE_MAX: f32 = 0.20;

/// Languages the UI has strings for.
pub const LANGUAGES: &[&str] = &["en", "vi"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractKey {
    Alt,
    Ctrl,
    Shift,
    Off,
}

/// User settings, stored as JSON. Unknown or missing fields fall back to the
/// defaults, so old files keep loading as fields are added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub style: String,
    pub pinned: Vec<String>,
    /// Hand length as a fraction of the screen height.
    pub pointer_size: f32,
    pub show_shadow: bool,
    /// Seconds before a drawing fades; 0 keeps it until cleared.
    pub fade_seconds: f32,
    pub hand_sound: bool,
    pub laser_sound: bool,
    pub tap_sound: String,
    pub interact_key: InteractKey,
    pub ink: String,
    pub welcomed: bool,
    /// UI language code ("en", "vi"); empty until the user picks one.
    pub language: String,
    /// Index into [`tools::PEN_SIZES`].
    pub pen_size: u8,
    /// Explain toolbar buttons when the pointer rests on them.
    pub show_hints: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            style: "lavender".into(),
            pinned: catalog::DEFAULT_PINNED.iter().map(|s| s.to_string()).collect(),
            pointer_size: 0.13,
            show_shadow: true,
            fade_seconds: 3.0,
            hand_sound: true,
            laser_sound: false,
            tap_sound: "thock".into(),
            interact_key: InteractKey::Alt,
            ink: "match".into(),
            welcomed: false,
            language: String::new(),
            pen_size: tools::DEFAULT_PEN_SIZE,
            show_hints: true,
        }
    }
}

impl Prefs {
    /// Clamp values and drop anything that no longer exists.
    pub fn sanitized(mut self) -> Self {
        let d = Prefs::default();
        if !catalog::is_style(&self.style) {
            self.style = d.style.clone();
        }
        let mut seen = Vec::new();
        self.pinned.retain(|p| catalog::is_style(p) && !seen.contains(p) && {
            seen.push(p.clone());
            true
        });
        self.pinned.truncate(catalog::MAX_PINNED);
        if self.pinned.is_empty() {
            self.pinned = d.pinned.clone();
        }
        if !self.pointer_size.is_finite() {
            self.pointer_size = d.pointer_size;
        }
        self.pointer_size = self.pointer_size.clamp(POINTER_SIZE_MIN, POINTER_SIZE_MAX);
        if !self.fade_seconds.is_finite() || self.fade_seconds < 0.0 {
            self.fade_seconds = d.fade_seconds;
        }
        if !catalog::SOUNDS.iter().any(|(id, _)| *id == self.tap_sound) {
            self.tap_sound = d.tap_sound.clone();
        }
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language.clear();
        }
        if !catalog::INKS.iter().any(|(id, _, _)| *id == self.ink) {
            self.ink = d.ink;
        }
        self.pen_size = self.pen_size.min(tools::PEN_SIZES.len() as u8 - 1);
        self
    }

    pub fn is_laser(&self) -> bool {
        self.style == catalog::LASER
    }

    pub fn sound_on(&self) -> bool {
        if self.is_laser() {
            self.laser_sound
        } else {
            self.hand_sound
        }
    }

    pub fn set_sound_on(&mut self, on: bool) {
        if self.is_laser() {
            self.laser_sound = on;
        } else {
            self.hand_sound = on;
        }
    }

    pub fn ink_rgb(&self) -> [u8; 3] {
        catalog::rgb(catalog::ink_color(&self.ink, &self.style))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_defaults() {
        let p: Prefs = serde_json::from_str(r#"{"style":"robot"}"#).unwrap_or_default();
        assert_eq!(p.style, "robot");
        assert_eq!(p.pointer_size, 0.13);
    }

    #[test]
    fn old_prefs_files_get_defaults() {
        let p: Prefs = serde_json::from_str(r#"{"style":"robot","fadeSeconds":5.0}"#).unwrap();
        assert_eq!(p.pen_size, tools::DEFAULT_PEN_SIZE);
        assert!(p.show_hints);
    }

    #[test]
    fn sanitize_limits_the_pen_size() {
        let q = Prefs { pen_size: 99, ..Prefs::default() }.sanitized();
        assert_eq!(q.pen_size, 3);
    }

    #[test]
    fn sanitize_drops_bad_values() {
        let p = Prefs {
            style: "nope".into(),
            pinned: vec!["robot".into(), "robot".into(), "x".into()],
            pointer_size: 9.0,
            ..Prefs::default()
        }
        .sanitized();
        assert_eq!(p.style, "lavender");
        assert_eq!(p.pinned, vec!["robot".to_string()]);
        assert_eq!(p.pointer_size, POINTER_SIZE_MAX);
    }
}
