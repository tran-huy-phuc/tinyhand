//! App state and the glue between the tray, shortcuts, windows and overlay.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tinyhand_core::{catalog, tools, Look, Prefs, Settings};

use crate::assets::Assets;
use crate::platform::{Overlay, OverlayConfig, OverlayEvent};
use crate::{i18n, log, sounds};

struct TrayItems {
    toggle: MenuItem<Wry>,
    clear: MenuItem<Wry>,
    settings: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

pub struct App {
    pub prefs: Mutex<Prefs>,
    prefs_path: PathBuf,
    pub overlay: Overlay,
    active: AtomicBool,
    pub assets: Assets,
    tray: Mutex<Option<TrayItems>>,
}

fn toggle_key() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH)
}

fn clear_key() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyX)
}

fn escape_key() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

const TRAY_ON: &[u8] = include_bytes!("../icons/tray_on.png");
const TRAY_OFF: &[u8] = include_bytes!("../icons/tray_off.png");

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // A menu bar app: no Dock icon.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    let handle = app.handle().clone();
    let data_dir = app.path().app_config_dir()?;
    log::init(data_dir.clone());
    log::write(format!("TinyHand {} (build {}) starting", app.package_info().version, env!("TINYHAND_BUILD")));

    let prefs_path = data_dir.join("prefs.json");
    let prefs = std::fs::read_to_string(&prefs_path)
        .ok()
        .and_then(|s| serde_json::from_str::<Prefs>(&s).ok())
        .unwrap_or_default()
        .sanitized();
    // Open Settings at launch until the welcome and language choice are done.
    let first_run = !prefs.welcomed || prefs.language.is_empty();

    let events_handle = handle.clone();
    let overlay = Overlay::new(move |event| match event {
        OverlayEvent::Reveal(on) => {
            let _ = events_handle.emit_to("toolbar", "reveal", on);
        }
        OverlayEvent::ToolbarPointer(at) => {
            let _ = events_handle.emit_to("toolbar", "pointer", at);
        }
    });

    app.manage(App {
        prefs: Mutex::new(prefs),
        prefs_path,
        overlay,
        active: AtomicBool::new(false),
        assets: Assets::new(app.path().resource_dir().ok()),
        tray: Mutex::new(None),
    });

    build_tray(&handle)?;
    let shortcuts = handle.global_shortcut();
    for s in [toggle_key(), clear_key()] {
        if let Err(e) = shortcuts.register(s) {
            log::write(format!("couldn't register {s:?}: {e}"));
        }
    }
    configure_overlay(&handle);
    ensure_toolbar(&handle);

    if first_run {
        open_settings(&handle);
    }
    // For trying the overlay from a dev build without touching the tray.
    #[cfg(debug_assertions)]
    if std::env::var_os("TINYHAND_POINTER_ON").is_some() {
        set_active(&handle, true);
    }
    Ok(())
}

pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<App>() {
        state.overlay.deactivate();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let text = i18n::text(&app.state::<App>().prefs.lock().unwrap().language);
    let toggle_item = MenuItem::with_id(app, "toggle", i18n::item(text.turn_on), true, None::<&str>)?;
    let clear = MenuItem::with_id(app, "clear", i18n::item(text.clear), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", text.settings, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", text.quit, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&toggle_item, &clear, &PredefinedMenuItem::separator(app)?, &settings, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    *app.state::<App>().tray.lock().unwrap() = Some(TrayItems { toggle: toggle_item, clear, settings, quit });

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(TRAY_OFF)?)
        .tooltip("TinyHand")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle(app),
            "clear" => app.state::<App>().overlay.clear_ink(),
            "settings" => open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn on_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    if *shortcut == toggle_key() {
        toggle(app);
    } else if *shortcut == clear_key() {
        app.state::<App>().overlay.clear_ink();
    } else if *shortcut == escape_key() {
        set_active(app, false);
    }
}

pub fn is_active(app: &AppHandle) -> bool {
    app.state::<App>().active.load(Ordering::SeqCst)
}

pub fn toggle(app: &AppHandle) {
    set_active(app, !is_active(app));
}

pub fn set_active(app: &AppHandle, on: bool) {
    let state = app.state::<App>();
    if state.active.swap(on, Ordering::SeqCst) == on {
        return;
    }
    log::write(if on { "pointer on" } else { "pointer off" });
    let shortcuts = app.global_shortcut();
    if on {
        let toolbar = ensure_toolbar(app);
        size_toolbar(app);
        state.overlay.set_pass_windows(pass_windows(app));
        state.overlay.activate(toolbar.as_ref().and_then(crate::platform::window_handle));
        // Esc only belongs to TinyHand while the pointer is on.
        if let Err(e) = shortcuts.register(escape_key()) {
            log::write(format!("couldn't register Esc: {e}"));
        }
        if !cfg!(any(windows, target_os = "macos")) {
            if let Some(t) = toolbar {
                let _ = t.show();
            }
        }
    } else {
        let _ = shortcuts.unregister(escape_key());
        state.overlay.deactivate();
        configure_overlay(app);
        if !cfg!(any(windows, target_os = "macos")) {
            if let Some(t) = app.get_webview_window("toolbar") {
                let _ = t.hide();
            }
        }
    }
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(icon) = Image::from_bytes(if on { TRAY_ON } else { TRAY_OFF }) {
            let _ = tray.set_icon(Some(icon));
        }
    }
    refresh_texts(app);
    emit_state(app);
}

/// Tray menu, tooltip and window titles in the current language.
fn refresh_texts(app: &AppHandle) {
    let state = app.state::<App>();
    let text = i18n::text(&state.prefs.lock().unwrap().language);
    let on = is_active(app);
    if let Some(items) = state.tray.lock().unwrap().as_ref() {
        let _ = items.toggle.set_text(i18n::item(if on { text.turn_off } else { text.turn_on }));
        let _ = items.clear.set_text(i18n::item(text.clear));
        let _ = items.settings.set_text(text.settings);
        let _ = items.quit.set_text(text.quit);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(if on { text.tooltip_on } else { "TinyHand" }));
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_title(text.settings_title);
    }
}

/// Windows whose clicks should go through to them while the pointer is on.
fn pass_windows(app: &AppHandle) -> Vec<isize> {
    app.get_webview_window("settings").as_ref().and_then(crate::platform::window_handle).into_iter().collect()
}

/// Size the toolbar from its contents up front. The page also measures
/// itself, but a hidden webview may not lay out until it's shown.
/// Keep in step with the CSS in ui/toolbar.html.
pub fn size_toolbar(app: &AppHandle) {
    let Some(w) = app.get_webview_window("toolbar") else { return };
    let state = app.state::<App>();
    let n = {
        let prefs = state.prefs.lock().unwrap();
        prefs.pinned.len() + usize::from(!prefs.pinned.contains(&prefs.style))
    } as f64;
    // body 14px padding, bar 10/8px padding + 1px border, 40px buttons,
    // 6px gaps. Pointer bar: the hands, a divider, sound and exit.
    let hands = 22.0 + n * 40.0 + 1.0 + 2.0 * 40.0 + (n + 2.0) * 6.0;
    // Tool bar: 2 buttons and a divider.
    let tools = 22.0 + 2.0 * 40.0 + 1.0 + 2.0 * 6.0;
    let width = 28.0 + f64::max(hands, tools);
    // The two bars, plus the space kept for the tray and hints (.top in the page).
    let height = 28.0 + 2.0 * 58.0 + 8.0 + 128.0 + 8.0;
    let _ = w.set_size(tauri::LogicalSize::new(width, height));
}

pub fn update_prefs(app: &AppHandle, change: impl FnOnce(&mut Prefs)) {
    let state = app.state::<App>();
    {
        let mut prefs = state.prefs.lock().unwrap();
        let mut next = prefs.clone();
        change(&mut next);
        *prefs = next.sanitized();
        if let Some(dir) = state.prefs_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&*prefs) {
            if let Err(e) = std::fs::write(&state.prefs_path, json) {
                log::write(format!("couldn't save prefs: {e}"));
            }
        }
    }
    configure_overlay(app);
    refresh_texts(app);
    emit_state(app);
}

/// Send the current prefs to the overlay. Decoding a hand takes ~50 ms, so
/// do it off the main thread.
pub fn configure_overlay(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<App>();
        let prefs = state.prefs.lock().unwrap().clone();
        let look = if prefs.is_laser() {
            Look::Laser
        } else {
            match state.assets.hand(&prefs.style) {
                Some(a) => Look::Hand(a),
                None => Look::Laser,
            }
        };
        state.overlay.configure(OverlayConfig {
            look,
            settings: Settings {
                size: prefs.pointer_size,
                shadow: prefs.show_shadow,
                fade_seconds: prefs.fade_seconds,
                ink: prefs.ink_rgb(),
                pen_scale: tools::pen_scale(prefs.pen_size),
            },
            sound: prefs.sound_on().then(|| sounds::bytes(&prefs.tap_sound)),
            interact: prefs.interact_key,
        });
    });
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StyleInfo {
    id: &'static str,
    title: &'static str,
    ink: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    active: bool,
    prefs: Prefs,
    styles: Vec<StyleInfo>,
    default_pinned: &'static [&'static str],
    sounds: Vec<(&'static str, &'static str)>,
    inks: Vec<(&'static str, &'static str, Option<String>)>,
    launch_at_login: bool,
    version: String,
    build: &'static str,
    platform_ready: bool,
}

pub fn ui_state(app: &AppHandle) -> UiState {
    let state = app.state::<App>();
    let hex = |c: u32| format!("#{c:06x}");
    let mut styles: Vec<StyleInfo> =
        catalog::HANDS.iter().map(|s| StyleInfo { id: s.id, title: s.title, ink: hex(s.ink) }).collect();
    styles.push(StyleInfo { id: catalog::LASER, title: catalog::title(catalog::LASER), ink: hex(catalog::RED) });
    let prefs = state.prefs.lock().unwrap().clone();
    UiState {
        active: is_active(app),
        prefs,
        styles,
        default_pinned: catalog::DEFAULT_PINNED,
        sounds: catalog::SOUNDS.to_vec(),
        inks: catalog::INKS.iter().map(|(id, t, c)| (*id, *t, c.map(hex))).collect(),
        launch_at_login: app.autolaunch().is_enabled().unwrap_or(false),
        version: app.package_info().version.to_string(),
        build: env!("TINYHAND_BUILD"),
        platform_ready: cfg!(any(windows, target_os = "macos")),
    }
}

pub fn emit_state(app: &AppHandle) {
    let _ = app.emit("state", ui_state(app));
}

pub fn ensure_toolbar(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window("toolbar") {
        return Some(w);
    }
    let builder = WebviewWindowBuilder::new(app, "toolbar", WebviewUrl::App("toolbar.html".into()))
        .title("TinyHand toolbar")
        .inner_size(420.0, 72.0)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .visible(false);
    #[cfg(any(windows, target_os = "macos"))]
    let builder = builder.transparent(true);
    #[cfg(target_os = "macos")]
    let builder = builder.accept_first_mouse(true);
    match builder.build() {
        Ok(w) => {
            crate::platform::prepare_toolbar(&w);
            Some(w)
        }
        Err(e) => {
            log::write(format!("couldn't create toolbar: {e}"));
            None
        }
    }
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let title = i18n::text(&app.state::<App>().prefs.lock().unwrap().language).settings_title;
    match WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title(title)
        .inner_size(640.0, 760.0)
        .min_inner_size(520.0, 520.0)
        .center()
        .build()
    {
        Ok(_) => {
            let state = app.state::<App>();
            state.overlay.set_pass_windows(pass_windows(app));
        }
        Err(e) => log::write(format!("couldn't open settings: {e}")),
    }
}
