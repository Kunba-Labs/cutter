//! Cuttar desktop shell: one window over the core. Everything the page does is
//! `invoke("dispatch", {action, args})`; the core says `library-changed` when
//! the page should re-read its snapshot. Window handling follows membox/inbox2.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use cuttar_core::Library;
use serde_json::Value;
use tauri::{Emitter, Manager, State};

mod updater;

pub(crate) struct Core(Arc<Library>);

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

fn ctx_identifier(dev: bool) -> String {
    if dev { format!("{}.dev", cuttar_core::BUNDLE_ID) } else { cuttar_core::BUNDLE_ID.to_string() }
}

fn main() {
    cuttar_core::tools::inherit_login_path();
    let ctx = tauri::generate_context!();
    let dev = ctx.config().identifier.ends_with(".dev");
    // Only a release-config build has an updater; see updater.rs.
    let builder = tauri::Builder::default();
    let builder = if ctx.config().plugins.0.contains_key("updater") { builder.plugin(tauri_plugin_updater::Builder::new().build()) } else { builder };
    builder
        .plugin(tauri_plugin_window_state::Builder::new().with_state_flags(tauri_plugin_window_state::StateFlags::all() & !tauri_plugin_window_state::StateFlags::MAXIMIZED).build())
        .setup(move |app| {
            let data_dir = cuttar_core::default_data_dir(dev);
            std::fs::create_dir_all(&data_dir)?;
            let log_path = cuttar_core::applog::init(&data_dir, log::LevelFilter::Info);
            // A second copy (the port is taken) hands over to the running one instead of panicking.
            let lib = match Library::open(&data_dir, Some(cuttar_core::default_port(dev))) {
                Ok(l) => l,
                Err(e) => {
                    log::error!("{e}");
                    let _ = std::process::Command::new("open").args(["-b", &ctx_identifier(dev)]).spawn();
                    std::process::exit(0);
                }
            };
            let h = app.handle().clone();
            lib.on_change(Box::new(move || {
                let _ = h.emit("library-changed", ());
            }));
            log::info!("cuttar data dir: {} · log: {}", data_dir.display(), log_path.display());
            app.manage(Core(lib));
            updater::poll(app.handle());
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("media", |_ctx, request, responder| {
            std::thread::spawn(move || responder.respond(media_response(&request)));
        })
        .invoke_handler(tauri::generate_handler![dispatch, open_url, updater::check_for_updates, updater::install_update])
        .run(ctx)
        .expect("error while running cuttar");
}

/// media://localhost/<path>: files for the webview's players and images. Tauri's asset protocol
/// caps a byte range at 1 MB, and WebKit cannot assemble a movie index bigger than one piece
/// (a 90-minute 60 fps lecture has an 8.6 MB index): the picture stays black while audio plays.
/// Pieces here go up to 64 MB. Only files under $HOME are served.
fn media_response(req: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    const CAP: u64 = 64 * 1024 * 1024;
    // CORS-clean for the preview's audio graph (the player reads it with crossOrigin="anonymous"):
    // a Range request is preflighted, and the preflight must say Range is fine.
    let reply = |status: u16| {
        tauri::http::Response::builder().status(status).header("Access-Control-Allow-Origin", "*").header("Access-Control-Allow-Headers", "Range").header("Access-Control-Allow-Methods", "GET, OPTIONS").header("Access-Control-Expose-Headers", "Content-Range, Content-Length, Accept-Ranges").header("Accept-Ranges", "bytes")
    };
    if req.method() == tauri::http::Method::OPTIONS {
        return reply(204).body(Vec::new()).unwrap();
    }
    let path = pct_decode(req.uri().path().trim_start_matches('/'));
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() || !path.starts_with(&format!("{home}/")) || path.contains("/../") {
        return reply(403).body(Vec::new()).unwrap();
    }
    let Ok(mut f) = std::fs::File::open(&path) else { return reply(404).body(Vec::new()).unwrap() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let ext = std::path::Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "srt" | "txt" | "ass" | "vtt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    // bytes=a-b, bytes=a-, bytes=-n (single range; WebKit asks for one at a time).
    let range = req.headers().get("range").and_then(|v| v.to_str().ok()).and_then(|r| r.trim().strip_prefix("bytes=")).and_then(|r| {
        let (a, b) = r.split(',').next()?.split_once('-')?;
        let (a, b) = (a.trim(), b.trim());
        if a.is_empty() {
            let n: u64 = b.parse().ok()?;
            Some((len.saturating_sub(n), len.saturating_sub(1)))
        } else {
            let start: u64 = a.parse().ok()?;
            let end: u64 = if b.is_empty() { len.saturating_sub(1) } else { b.parse().ok()? };
            Some((start, end))
        }
    });
    let (start, end, partial) = match range {
        Some((s, e)) => {
            if s >= len || e < s {
                return reply(416).header("Content-Range", format!("bytes */{len}")).body(Vec::new()).unwrap();
            }
            (s, e.min(len - 1).min(s + CAP - 1), true)
        }
        None if len > CAP => (0, CAP - 1, true),
        None => (0, len.saturating_sub(1), false),
    };
    let mut buf = Vec::with_capacity((end + 1 - start) as usize);
    if len > 0 && (f.seek(SeekFrom::Start(start)).is_err() || f.take(end + 1 - start).read_to_end(&mut buf).is_err()) {
        return reply(500).body(Vec::new()).unwrap();
    }
    let mut r = reply(if partial { 206 } else { 200 }).header("Content-Type", mime).header("Content-Length", buf.len().to_string());
    if partial {
        r = r.header("Content-Range", format!("bytes {start}-{end}/{len}"));
    }
    r.body(buf).unwrap()
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_serves_ranges_under_home() {
        let home = std::env::var("HOME").unwrap();
        let dir = std::path::PathBuf::from(&home).join(".cuttar-media-test");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a b.mp4");
        std::fs::write(&f, b"0123456789").unwrap();
        let uri = format!("media://localhost/{}", f.display().to_string().replace('/', "%2F").replace(' ', "%20"));
        let get = |range: Option<&str>| {
            let mut r = tauri::http::Request::builder().uri(&uri);
            if let Some(v) = range {
                r = r.header("range", v);
            }
            media_response(&r.body(Vec::new()).unwrap())
        };
        let r = get(Some("bytes=2-4"));
        assert_eq!((r.status().as_u16(), r.body().as_slice()), (206, &b"234"[..]));
        assert_eq!(r.headers()["content-range"], "bytes 2-4/10");
        assert_eq!(get(Some("bytes=7-")).body().as_slice(), b"789");
        assert_eq!(get(Some("bytes=-2")).body().as_slice(), b"89");
        assert_eq!(get(None).status().as_u16(), 200);
        assert_eq!(get(Some("bytes=20-")).status().as_u16(), 416);
        let outside = tauri::http::Request::builder().uri("media://localhost/%2Fetc%2Fhosts").body(Vec::new()).unwrap();
        assert_eq!(media_response(&outside).status().as_u16(), 403);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
