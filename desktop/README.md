# TinyHand for Windows and macOS (Rust + Tauri)

Cross-platform TinyHand. The Swift app in `../Sources` is the original Mac
build and stays until this one has been proven on real Macs and has what it
still lacks (see "Known gaps").

```
desktop/
  core/   tinyhand-core: pure Rust, no OS calls. Hand image measuring, arm
          physics, ink, prefs, catalog, and the renderer. Unit-tested.
  app/    tinyhand: the Tauri app. Tray, global shortcuts, settings/toolbar
          windows, and platform/ (the OS-specific overlay).
  ui/     toolbar.html and settings.html (plain HTML/JS, no bundler).
```

## Drawing tools

All in `core`, so both platforms behave the same:

| Tool | Where | Notes |
|---|---|---|
| Pen, thickness | `engine.rs`, `ink.rs` | Strokes fade after a few seconds, or stay until cleared |

Drawings are kept per monitor, so moving the cursor to another screen no longer
wipes them. They only show on the monitor the cursor is on, though.

## How it works on each OS

| macOS (`app/src/platform/macos`, after `../Sources`) | Windows (`app/src/platform/windows`) |
|---|---|
| `CGEventTap` + Accessibility permission | `WH_MOUSE_LL` low-level hook (`hook.rs`). No permission needed |
| `NSWindow` at screenSaver level showing two IOSurfaces in turn, painted by the same engine off the main thread | Layered `WS_EX_TRANSPARENT \| TOPMOST` window, painted by `tinyhand_core::Engine` via `UpdateLayeredWindowIndirect` with a dirty rect (`overlay.rs`) |
| `CGDisplayHideCursor` + private CGS calls | Every system cursor swapped for a blank one, restored with `SPI_SETCURSORS` on exit, on panic and on next launch (`cursor.rs`) |
| Carbon hotkeys | `tauri-plugin-global-shortcut` (Ctrl+Alt+H, Ctrl+Alt+X, Esc while on) |
| Menu bar item | Tray icon (left click toggles, right click menu) |
| Dock / menu bar pass-through | Taskbar strips, the toolbar and the Settings window pass clicks through |
| Hold ⌥ to click for real | Hold Alt (or Ctrl/Shift). Alt-clicks are followed by a dummy key so Alt doesn't open app menus |
| cpal (`audio.rs`) | cpal (`audio.rs`) |
| `tauri-plugin-autostart` (LaunchAgent) | `tauri-plugin-autostart` (HKCU Run key) |

The overlay thread sleeps on a channel: a click wakes it at once. While the
mouse moves or something animates, it ticks at the monitor's refresh rate,
and at 20 Hz when idle. Only the changed rectangle is re-rendered. The hand
and rod use a small span rasteriser (`core/src/raster.rs`) that only visits
covered pixels. Ink and the laser dot use tiny-skia.

Rendering cost per moving frame (Apple M-series, `cargo run -p tinyhand-core --release --example bench`):
about 1.6 ms at 1920×1080 and 6.4 ms at 3840×2160, with shadow.

## Build

macOS (native; `--bundles dmg` also works, and needs a logged-in session):

```sh
cd desktop/app
npx @tauri-apps/cli@2 dev                 # run
npx @tauri-apps/cli@2 build --bundles app # desktop/target/release/bundle/macos/TinyHand.app
```

Builds are ad-hoc signed by default. Set `APPLE_SIGNING_IDENTITY` to a
Developer ID Application certificate to distribute one. A dev build also reads
`TINYHAND_POINTER_ON`, to start with the pointer on.

Windows (native):

```powershell
cd desktop\app
npx @tauri-apps/cli@2 dev      # run
npx @tauri-apps/cli@2 build    # NSIS installer in desktop\target\release\bundle\nsis
```

From macOS (cross-compile; links a real `.exe`, can't run it):

```sh
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin
cd desktop && cargo xwin build -p tinyhand --release --target x86_64-pc-windows-msvc
```

CI (`.github/workflows/windows.yml`) tests the core and uploads the installer as an artifact. Pushing a `windows-v1.2.3` tag also publishes it as a GitHub release; bump the version in `app/tauri.conf.json` and `app/Cargo.toml` first.

Core only, any OS:

```sh
cd desktop
cargo test -p tinyhand-core
cargo run -p tinyhand-core --release --example render -- robot frame.png   # render a frame to PNG
cargo run -p tinyhand-core --release --example bench
```

`cargo run -p tinyhand` on macOS starts the tray, Settings and toolbar with a
stub overlay, which is handy for UI work.

## Known gaps / next steps

- Over windows running as admin (Task Manager, elevated terminals), Windows
  doesn't let a normal app's hook see the clicks, so they go through. Staying
  above the Start menu also needs `uiAccess="true"`, which needs a signed
  build installed under Program Files.
- Apps that draw their own custom cursor (some games, design tools) still
  show it. A fallback is the Magnification API's `MagShowSystemCursor`.
- Sign the installer (e.g. Azure Trusted Signing). An unsigned app that hooks
  the mouse and hides the cursor will get SmartScreen and antivirus warnings.
- Still missing compared to the Swift app, on both OSes: custom hands
  (HandMaker), the first-run hint bubble, editable shortcuts, and auto-update
  (`tauri-plugin-updater`).
- macOS: not code-signed with a Developer ID or notarized. Esc stays taken
  while a menu is open, where the Swift app gives it back.
- Performance headroom: on 4K screens a DirectComposition backend (GPU
  transforms, like Core Animation on the Mac) would make frames nearly free.
