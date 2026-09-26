//! Cuttar desktop shell: one window over the core. Everything the page does is
//! `invoke("dispatch", {action, args})`; the core says `library-changed` when
//! the page should re-read its snapshot. Window handling follows membox/inbox2.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use cuttar_core::Library;
use serde_json::Value;
use tauri::{Emitter, Manager, State};

struct Core(Arc<Library>);

/// Async on purpose: a sync command runs on the main thread and a long action
/// would freeze the window.
#[tauri::command]
async fn dispatch(core: State<'_, Core>, action: String, args: Value) -> Result<Value, String> {
    let lib = core.0.clone();
    tauri::async_runtime::spawn_blocking(move || lib.dispatch(&action, args)).await.map_err(|e| e.to_string())?
}

/// WKWebView swallows window.open; hand links to the OS.
#[tauri::command]
fn open_url(url: String) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

fn main() {
    cuttar_core::tools::inherit_login_path();
    let ctx = tauri::generate_context!();
    let dev = ctx.config().identifier.ends_with(".dev");
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().with_state_flags(tauri_plugin_window_state::StateFlags::all() & !tauri_plugin_window_state::StateFlags::MAXIMIZED).build())
        .setup(move |app| {
            let data_dir = cuttar_core::default_data_dir(dev);
            std::fs::create_dir_all(&data_dir)?;
            let log_path = cuttar_core::applog::init(&data_dir, log::LevelFilter::Info);
            let lib = Library::open(&data_dir, Some(cuttar_core::default_port(dev))).map_err(std::io::Error::other)?;
            let h = app.handle().clone();
            lib.on_change(Box::new(move || {
                let _ = h.emit("library-changed", ());
            }));
            log::info!("cuttar data dir: {} · log: {}", data_dir.display(), log_path.display());
            app.manage(Core(lib));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![dispatch, open_url])
        .run(ctx)
        .expect("error while running cuttar");
}
