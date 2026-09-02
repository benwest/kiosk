mod server;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

const DEFAULT_PORT: u16 = 8973;

const MAIN_INIT_SCRIPT: &str = r#"
(function () {
  window.addEventListener(
    "keydown",
    function (e) {
      // e.key is the composed character (Option+Shift+C types "Ç" on macOS,
      // not "c"), so check the physical key via e.code instead.
      if (e.metaKey && e.altKey && e.shiftKey && e.code === "KeyC") {
        e.preventDefault();
        // window.__TAURI__ (the withGlobalTauri wrapper) isn't reliably present on
        // externally-loaded content; __TAURI_INTERNALS__ is the underlying IPC
        // bridge every Tauri webview gets regardless of origin.
        if (window.__TAURI_INTERNALS__) {
          window.__TAURI_INTERNALS__.invoke("toggle_config");
        }
      }
    },
    true
  );
})();
"#;

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    content_path: String,
    port: u16,
}

#[derive(Default)]
struct AppState {
    server: Mutex<Option<server::ServerHandle>>,
}

fn config_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("app data dir unavailable")
        .join("config.json")
}

fn load_config(app: &AppHandle) -> Option<Config> {
    let data = std::fs::read_to_string(config_path(app)).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_config(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let path = config_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg).unwrap()).map_err(|e| e.to_string())
}

fn create_main_window(app: &AppHandle, url: &str) -> tauri::Result<()> {
    let monitor = app
        .primary_monitor()?
        .expect("no primary monitor detected");
    // Monitor size/position are physical pixels; the builder's position/inner_size
    // take logical points, so convert or Retina displays get a 2x-oversized window.
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let position = monitor.position().to_logical::<f64>(scale);

    WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.parse().unwrap()))
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .position(position.x, position.y)
        .inner_size(size.width, size.height)
        .initialization_script(MAIN_INIT_SCRIPT)
        .visible(true)
        .build()?;
    Ok(())
}

fn create_config_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, "config", WebviewUrl::App("index.html".into()))
        .title("Kiosk Config")
        .inner_size(420.0, 340.0)
        .resizable(false)
        .always_on_top(true)
        .visible(false)
        .build()?;
    Ok(())
}

/// Stops any running server, starts a new one for `cfg`, and points the main
/// window (creating it on first run) at it.
fn apply_config(app: &AppHandle, state: &State<AppState>, cfg: &Config) -> Result<(), String> {
    let mut guard = state.server.lock().unwrap();
    if let Some(old) = guard.take() {
        old.stop();
    }
    let handle = server::start(cfg.content_path.clone(), cfg.port)?;
    *guard = Some(handle);
    drop(guard);

    let url = format!("http://127.0.0.1:{}", cfg.port);
    if let Some(win) = app.get_webview_window("main") {
        let parsed: tauri::Url = url.parse().map_err(|e| format!("{e}"))?;
        win.navigate(parsed).map_err(|e| e.to_string())?;
    } else {
        create_main_window(app, &url).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_config(app: AppHandle) -> Option<Config> {
    load_config(&app)
}

#[tauri::command]
fn save_config(
    app: AppHandle,
    state: State<AppState>,
    content_path: String,
    port: Option<u16>,
) -> Result<(), String> {
    let cfg = Config {
        content_path,
        port: port.unwrap_or(DEFAULT_PORT),
    };
    write_config(&app, &cfg)?;
    apply_config(&app, &state, &cfg)?;
    if let Some(win) = app.get_webview_window("config") {
        win.hide().ok();
    }
    Ok(())
}

#[tauri::command]
fn toggle_config(app: AppHandle) {
    if let Some(win) = app.get_webview_window("config") {
        let visible = win.is_visible().unwrap_or(false);
        if visible {
            win.hide().ok();
        } else {
            win.show().ok();
            win.set_focus().ok();
            win.emit("config-shown", ()).ok();
        }
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

// Must not block the calling thread: on macOS the picker's own modal loop
// runs on the main thread, so `blocking_pick_folder` there deadlocks the UI.
#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    rx.await.ok().flatten().map(|p| p.to_string())
}

// A window below the menu bar's level can never draw over that strip no
// matter its position/size; the system reserves it. Hiding it via
// presentation options (distinct from native Fullscreen) frees that space
// without the Spaces-based hover-reveal gesture the whole app exists to avoid.
#[cfg(target_os = "macos")]
fn hide_menu_bar_and_dock() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions};

    let mtm = MainThreadMarker::new().expect("must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setPresentationOptions(
        NSApplicationPresentationOptions::HideMenuBar
            | NSApplicationPresentationOptions::HideDock,
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            toggle_config,
            quit_app,
            pick_folder
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            #[cfg(target_os = "macos")]
            hide_menu_bar_and_dock();

            create_config_window(&handle)?;

            let state = handle.state::<AppState>();
            match load_config(&handle) {
                Some(cfg) => {
                    apply_config(&handle, &state, &cfg)?;
                }
                None => {
                    if let Some(win) = handle.get_webview_window("config") {
                        win.show()?;
                        win.set_focus()?;
                    }
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
