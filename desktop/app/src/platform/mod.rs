//! OS-specific pieces: the click-swallowing mouse hook, the transparent
//! overlay window, hiding the system cursor, and tap sounds.
//!
//! Windows and macOS are implemented. Other platforms get a stub so the app
//! (tray, settings, toolbar) still builds and runs for UI work.

use std::sync::atomic::{AtomicU32, Ordering};

use tinyhand_core::prefs::InteractKey;
use tinyhand_core::{Look, Settings};

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub struct OverlayConfig {
    pub look: Look,
    pub settings: Settings,
    /// `None` when muted.
    pub sound: Option<&'static [u8]>,
    pub interact: InteractKey,
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub enum OverlayEvent {
    /// Show or fade out the toolbar (it hides while you're away from it).
    Reveal(bool),
    /// Where the cursor is over the toolbar window, as fractions of its size;
    /// `None` once it has left. The platforms track this themselves because a
    /// page in a window that never takes focus can't count on hover events.
    ToolbarPointer(Option<(f32, f32)>),
}

/// How far down the toolbar window (in thousandths of its height) its visible
/// part starts. The space above is kept for the tray and hints, so the window
/// needn't resize and jump; clicks there should act like clicks anywhere else.
static TOOLBAR_SOLID_TOP: AtomicU32 = AtomicU32::new(0);

pub fn set_toolbar_solid_top(fraction: f64) {
    TOOLBAR_SOLID_TOP.store((fraction.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub fn toolbar_solid_top() -> f64 {
    TOOLBAR_SOLID_TOP.load(Ordering::Relaxed) as f64 / 1000.0
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

#[cfg(not(any(windows, target_os = "macos")))]
mod other;
#[cfg(not(any(windows, target_os = "macos")))]
pub use self::other::*;
