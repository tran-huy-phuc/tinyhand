//! Commands the toolbar and settings pages call.

use tauri::{AppHandle, LogicalSize, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tinyhand_core::{catalog, Prefs};

use crate::app::{self, UiState};

#[tauri::command]
pub fn get_state(app: AppHandle) -> UiState {
    app::ui_state(&app)
}

#[tauri::command]
pub fn set_prefs(app: AppHandle, prefs: Prefs) {
    app::update_prefs(&app, |p| {
        // The page doesn't own this flag; keep it.
        let welcomed = p.welcomed || prefs.welcomed;
        *p = prefs;
        p.welcomed = welcomed;
    });
}

#[tauri::command]
pub fn reset_prefs(app: AppHandle) {
    app::update_prefs(&app, |p| {
        // Keep the language: resetting shouldn't ask again.
        *p = Prefs { welcomed: true, language: p.language.clone(), ..Prefs::default() };
    });
}

#[tauri::command]
pub fn pick_style(app: AppHandle, id: String) {
    if catalog::is_style(&id) {
        app::update_prefs(&app, |p| p.style = id);
    }
}

#[tauri::command]
pub fn toggle_sound(app: AppHandle) {
    app::update_prefs(&app, |p| {
        let on = p.sound_on();
        p.set_sound_on(!on);
    });
}

#[tauri::command]
pub fn set_active(app: AppHandle, on: bool) {
    app::set_active(&app, on);
}

#[tauri::command]
pub fn clear_ink(app: AppHandle) {
    app.state::<app::App>().overlay.clear_ink();
}

#[tauri::command]
pub fn set_pen_size(app: AppHandle, size: u8) {
    app::update_prefs(&app, |p| p.pen_size = size);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    app::open_settings(&app);
}

#[tauri::command]
pub fn preview_sound(id: String) {
    crate::audio::play(crate::sounds::bytes(&id));
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, on: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    let result = if on { auto.enable() } else { auto.disable() };
    app::emit_state(&app);
    result.map_err(|e| e.to_string())
}

/// The toolbar page reports where its visible part starts.
#[tauri::command]
pub fn toolbar_solid_top(fraction: f64) {
    crate::platform::set_toolbar_solid_top(fraction);
}

/// The toolbar page reports its content size; the overlay keeps it centred.
#[tauri::command]
pub fn toolbar_resize(app: AppHandle, width: f64, height: f64) {
    if let Some(w) = app.get_webview_window("toolbar") {
        let _ = w.set_size(LogicalSize::new(width.clamp(80.0, 1600.0), height.clamp(40.0, 400.0)));
    }
}
