#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod audio;
mod commands;
mod i18n;
mod log;
mod platform;
mod sounds;

use tauri::{Manager, RunEvent};
use tauri_plugin_global_shortcut::ShortcutState;

fn main() {
    platform::early_init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Launching again opens Settings instead of a second pointer.
            app::open_settings(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        app::on_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let id = request.uri().path().trim_start_matches('/').to_string();
            std::thread::spawn(move || {
                let state = app.state::<app::App>();
                let response = match state.assets.thumbnail(&id) {
                    Some(png) => tauri::http::Response::builder()
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "max-age=31536000")
                        .body(png),
                    None => tauri::http::Response::builder().status(404).body(Vec::new()),
                };
                responder.respond(response.expect("valid response"));
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_prefs,
            commands::reset_prefs,
            commands::pick_style,
            commands::toggle_sound,
            commands::set_active,
            commands::clear_ink,
            commands::set_pen_size,
            commands::open_settings,
            commands::preview_sound,
            commands::set_launch_at_login,
            commands::toolbar_resize,
            commands::toolbar_solid_top,
        ])
        .setup(|app| {
            app::setup(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("TinyHand failed to start");

    app.run(|app, event| match event {
        // No windows is normal: TinyHand lives in the tray.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            app::shutdown(app);
            platform::shutdown();
        }
        _ => {}
    });
}
