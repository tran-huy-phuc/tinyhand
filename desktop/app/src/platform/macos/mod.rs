//! macOS implementation.
//!
//! * `tap`: a `CGEventTap` that swallows clicks while the pointer is on. Needs
//!   the Accessibility permission.
//! * `overlay`: the frame loop, painting IOSurfaces with the shared
//!   `tinyhand_core::Engine`.
//! * `ui`: the AppKit windows: overlay and the toolbar panel.
//! * `cursor`: hides the system cursor, using private CoreGraphics calls.
//! * `ffi`: the C declarations for all of that.

mod cursor;
mod ffi;
mod overlay;
mod tap;
mod ui;

pub use overlay::Overlay;

pub fn early_init() {
    ffi::on_main(cursor::enable_background_hiding);
}

/// The system brings the cursor back when the process ends. This covers an
/// orderly quit while the event loop can still run it.
pub fn shutdown() {
    cursor::show();
}

pub fn play_sound(wav: &'static [u8]) {
    crate::audio::play(wav);
}

pub fn prepare_toolbar(window: &tauri::WebviewWindow) {
    if let Ok(ptr) = window.ns_window() {
        ui::prepare_toolbar(ptr);
    }
}

pub fn window_handle(window: &tauri::WebviewWindow) -> Option<isize> {
    window.ns_window().ok().map(|p| p as isize)
}
