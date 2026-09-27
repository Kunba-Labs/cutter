//! Loopback HTTP on a fixed port: `/mcp` (JSON-RPC, MCP streamable HTTP,
//! JSON-only) for Claude, `/dispatch` for the CLI, `/health`. Bearer token
//! persisted in the data dir so `claude mcp add` stays valid across launches.
//! Transport and origin/host gates lifted from membox (inbox2 before it).

use std::io::Cursor;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::Library;

#[derive(Debug, Deserialize)]
pub struct Rpc {
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub at: String,
    pub tool: String,
    pub args: Value,
    pub ok: bool,
}

pub struct McpServer {
    lib: Arc<Library>,
    pub token: String,
    pub port: u16,
    pub audit: Mutex<Vec<AuditEntry>>,
}

/// name, description, JSON schema properties, required, dispatch action.
/// A tool IS a dispatch action with a description: nothing to keep in step.
pub const TOOLS: &[(&str, &str, &str, &str, &str)] = &[
    ("list_library", "Everything: sources with stages, channels, inbox, candidates, renders, posts, posters, jobs, settings, tool availability.", r#"{}"#, "", "snapshot"),
    ("get_source", "One source with its transcript (timed segments), reel candidates and renders.", r#"{"id":{"type":"string"}}"#, "id", "source"),
    ("add_source", "Add a YouTube URL or a local video file. Starts download → transcript → reel finding. language: ISO code or omit for auto. transcriptSource: whisper|captions|both.", r#"{"url":{"type":"string"},"path":{"type":"string"},"language":{"type":"string"},"transcriptSource":{"type":"string"},"autoDetect":{"type":"boolean"},"autoApproveScore":{"type":"integer"}}"#, "", "add_source"),
    ("run_stage", "Re-run a stage for a source: download | transcribe | detect.", r#"{"id":{"type":"string"},"stage":{"type":"string"}}"#, "id", "run"),
    ("edit_segment", "Correct one transcript line by index; same word count keeps the timings.", r#"{"id":{"type":"string"},"index":{"type":"integer"},"text":{"type":"string"}}"#, "id,index,text", "edit_segment"),
    ("edit_word", "Replace one word of a transcript line (index = line, word = position in the line); timing untouched.", r#"{"id":{"type":"string"},"index":{"type":"integer"},"word":{"type":"integer"},"text":{"type":"string"}}"#, "id,index,word,text", "edit_word"),
    ("polish_captions", "Have the brain fix clear transcript errors word for word (keeps timings, ≤5% of words); thenDetect re-runs the reel search after.", r#"{"id":{"type":"string"},"thenDetect":{"type":"boolean"}}"#, "id", "polish"),
    ("set_language", "Override the spoken language of a source (ISO code, or 'auto') and transcribe again.", r#"{"id":{"type":"string"},"language":{"type":"string"},"retranscribe":{"type":"boolean"}}"#, "id,language", "set_language"),
    ("search_transcripts", "Full-text search over every transcript; returns source ids.", r#"{"query":{"type":"string"}}"#, "query", "search"),
    ("update_candidate", "Edit a reel candidate: patch may hold start, end, title, hook, caption, hashtags, category, crop {x,y}, style, formats.", r#"{"id":{"type":"string"},"patch":{"type":"object"}}"#, "id,patch", "update_candidate"),
    ("add_candidate", "Hand-pick a reel by time range on a source.", r#"{"sourceId":{"type":"string"},"start":{"type":"number"},"end":{"type":"number"},"title":{"type":"string"}}"#, "sourceId,start,end", "add_candidate"),
    ("apply_look", "Copy one reel's adjustments (keys: style, hookStyle, captionPct, crop, formats) onto others; scope: source | approved | library.", r#"{"id":{"type":"string"},"keys":{"type":"array","items":{"type":"string"}},"scope":{"type":"string"}}"#, "id", "apply_look"),
    ("approve", "Approve (or un-approve) candidates by id.", r#"{"ids":{"type":"array","items":{"type":"string"}},"approved":{"type":"boolean"}}"#, "ids", "approve"),
    ("approve_above", "Approve every candidate of a source scoring at least `score`.", r#"{"sourceId":{"type":"string"},"score":{"type":"integer"}}"#, "sourceId", "approve_above"),
    ("discard", "Discard a candidate (toggle).", r#"{"id":{"type":"string"}}"#, "id", "discard"),
    ("render", "Render reels: candidateIds, or sourceId for all approved. formats: shorts|reels|tiktok|feed|landscape.", r#"{"candidateIds":{"type":"array","items":{"type":"string"}},"sourceId":{"type":"string"},"formats":{"type":"array","items":{"type":"string"}}}"#, "", "render"),
    ("list_jobs", "The job queue with progress and messages.", r#"{}"#, "", "jobs"),
    ("set_thumbnail", "Send the reel's cover as the thumbnail of a posted YouTube video. id = post id.", r#"{"id":{"type":"string"}}"#, "id", "set_thumbnail"),
    ("set_privacy", "Change privacy on a posted YouTube video: id = post id, privacy public|unlisted|private.", r#"{"id":{"type":"string"},"privacy":{"type":"string"}}"#, "id", "set_privacy"),
    ("add_target", "Add a place to post: kind youtube|instagram|tiktok|facebook|folder, optional name, path (folder), hours.", r#"{"kind":{"type":"string"},"name":{"type":"string"},"path":{"type":"string"},"hours":{"type":"array","items":{"type":"integer"}}}"#, "kind", "add_target"),
    ("update_target", "Change a target: patch with name, enabled, hours, format, autoSchedule, privacy, path…", r#"{"id":{"type":"string"},"patch":{"type":"object"}}"#, "id,patch", "update_target"),
    ("remove_target", "Remove a target.", r#"{"id":{"type":"string"}}"#, "id", "remove_target"),
    ("target_connect", "Connect a YouTube target: the browser opens for Google's consent.", r#"{"id":{"type":"string"}}"#, "id", "target_connect"),
    ("cancel_jobs", "Stop every queued or running job, optionally of one kind (render, publish…).", r#"{"kind":{"type":"string"}}"#, "", "cancel_jobs"),
    ("cancel_job", "Cancel a queued or running job.", r#"{"id":{"type":"string"}}"#, "id", "cancel_job"),
    ("retry_job", "Queue a failed job again.", r#"{"id":{"type":"string"}}"#, "id", "retry_job"),
    ("pick_cover", "Choose a reel's cover frame again (sharpness pass + brain) and render its formats again.", r#"{"id":{"type":"string"}}"#, "id", "pick_cover"),
    ("verify_renders", "Measure finished renders (size, fps, bitrate, length, audio) and flag shortfalls. sourceId, candidateIds, or none for all.", r#"{"sourceId":{"type":"string"},"candidateIds":{"type":"array","items":{"type":"string"}}}"#, "", "verify_renders"),
    ("covers", "Redo the cover images of finished renders (after a title or frame change); no re-encode. sourceId or candidateIds.", r#"{"sourceId":{"type":"string"},"candidateIds":{"type":"array","items":{"type":"string"}}}"#, "", "covers"),
    ("redownload", "Fetch a YouTube source's best stream again; transcript, reels and edits stay.", r#"{"id":{"type":"string"}}"#, "id", "redownload"),
    ("remove_job", "Drop one job from the queue, whatever its state.", r#"{"id":{"type":"string"}}"#, "id", "remove_job"),
    ("add_channel", "Watch a YouTube channel or playlist URL. intervalH, minLenS, titleRegex, since (YYYY-MM-DD), autoProcess, autoApproveScore, autoSchedule.", r#"{"url":{"type":"string"},"name":{"type":"string"},"intervalH":{"type":"integer"},"minLenS":{"type":"integer"},"titleRegex":{"type":"string"},"since":{"type":"string"},"autoProcess":{"type":"boolean"},"autoApproveScore":{"type":"integer"},"autoSchedule":{"type":"boolean"}}"#, "url", "add_channel"),
    ("update_channel", "Change a watched channel's rules (patch).", r#"{"id":{"type":"string"},"patch":{"type":"object"}}"#, "id,patch", "update_channel"),
    ("check_channels", "Check one channel (id) or all of them for new uploads now.", r#"{"id":{"type":"string"}}"#, "", "check_channel"),
    ("process_inbox", "Turn a found upload into a source and run the pipeline.", r#"{"id":{"type":"string"}}"#, "id", "process_inbox"),
    ("schedule_post", "Put a rendered reel (candidateId or renderId) or a poster (posterId) on the calendar: channel youtube|instagram|tiktok|facebook, at = RFC 3339. confirmed=true sends it.", r#"{"candidateId":{"type":"string"},"renderId":{"type":"string"},"posterId":{"type":"string"},"channel":{"type":"string"},"at":{"type":"string"},"title":{"type":"string"},"caption":{"type":"string"},"confirmed":{"type":"boolean"}}"#, "channel,at", "schedule"),
    ("confirm_post", "Confirm a planned post so it goes out at its time.", r#"{"id":{"type":"string"}}"#, "id", "confirm"),
    ("publish_now", "Send a post right now.", r#"{"id":{"type":"string"}}"#, "id", "publish_now"),
    ("unschedule", "Remove a post from the calendar.", r#"{"id":{"type":"string"}}"#, "id", "unschedule"),
    ("autofill_calendar", "Place every rendered, unscheduled reel on the coming days by the cadence in settings.", r#"{"days":{"type":"integer"}}"#, "", "autofill"),
    ("create_poster", "Start a poster: template, title, fields {date, speaker, programme, location, qrLink, qrCaption, brief…}.", r#"{"template":{"type":"string"},"title":{"type":"string"},"fields":{"type":"object"}}"#, "title", "create_poster"),
    ("generate_poster", "Have Claude + Higgsfield draw 3 variants for a poster (a job).", r#"{"id":{"type":"string"}}"#, "id", "generate_poster"),
    ("add_poster_variant", "Add an image file as a variant; the QR is pasted in if a link is set.", r#"{"id":{"type":"string"},"path":{"type":"string"}}"#, "id,path", "add_variant"),
    ("choose_variant", "Pick the variant to export.", r#"{"id":{"type":"string"},"path":{"type":"string"}}"#, "id,path", "choose_variant"),
    ("export_poster", "Print PNG, 1600 px JPEG, feed 4:5 and story 9:16 of the chosen variant. palette = a few words to remember, so the next run avoids it.", r#"{"id":{"type":"string"},"palette":{"type":"string"}}"#, "id", "export_poster"),
    ("end_cards", "Draw the reel end cards (9:16, 4:5, 16:9) in a poster's style (a job).", r#"{"id":{"type":"string"}}"#, "id", "end_cards"),
    ("use_end_cards", "Use a poster's end cards on every render: enabled, seconds.", r#"{"id":{"type":"string"},"enabled":{"type":"boolean"},"seconds":{"type":"number"}}"#, "id", "use_end_cards"),
    ("waiting_video", "Render the 'starting soon' loops (16:9 and 9:16) plus break/ended stills from the chosen poster. startAt RFC 3339 for the countdown.", r#"{"id":{"type":"string"},"startAt":{"type":"string"},"lines":{"type":"array","items":{"type":"string"}},"loopS":{"type":"integer"},"countdown":{"type":"boolean"}}"#, "id", "waiting_video"),
    ("settings", "Read settings, or change them with patch.", r#"{"patch":{"type":"object"}}"#, "", "settings"),
    ("shutdown", "Drain and quit the app: no new jobs start, active ones finish, then it exits (queued work resumes next start).", r#"{"waitSeconds":{"type":"integer"}}"#, "", "shutdown"),
    ("dispatch", "Escape hatch: any core action by name with raw args (see the app's action list).", r#"{"action":{"type":"string"},"args":{"type":"object"}}"#, "action", "*"),
];

impl McpServer {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }

    pub fn spawn(lib: Arc<Library>, port: u16) -> Result<Arc<McpServer>, String> {
        let http = Server::http(format!("127.0.0.1:{port}")).map_err(|e| format!("port {port} is taken — is Cuttar already running? ({e})"))?;
        let token_path = lib.data_dir.join("mcp-token");
        let token = match std::fs::read_to_string(&token_path) {
            Ok(t) if t.trim().len() >= 32 => t.trim().to_string(),
            _ => {
                let t = crate::model::hex(&rand::random::<[u8; 24]>());
                let _ = std::fs::write(&token_path, &t);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600));
                }
                t
            }
        };
        let server = Arc::new(McpServer { lib, token, port, audit: Mutex::new(Vec::new()) });
        let s2 = server.clone();
        std::thread::Builder::new()
            .name("cuttar-mcp".into())
            .spawn(move || {
                for req in http.incoming_requests() {
                    let s3 = s2.clone();
                    // Each request on its own thread: a `dispatch` can take a while.
                    std::thread::spawn(move || handle(&s3, req));
                }
            })
            .map_err(|e| e.to_string())?;
        log::info!("cuttar MCP on {}", server.url());
        let cfg = json!({ "mcpServers": { "cuttar": { "type": "http", "url": server.url(), "headers": { "Authorization": format!("Bearer {}", server.token) } } }, "claude_mcp_add": format!("claude mcp add --transport http cuttar {} --header \"Authorization: Bearer {}\"", server.url(), server.token) });
        let path = server.lib.data_dir.join("mcp.json");
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(server)
    }

    pub fn handle_rpc(&self, rpc: &Rpc) -> Option<Value> {
        let id = rpc.id.clone()?;
        let result = match rpc.method.as_str() {
            "initialize" => Ok(json!({
                "protocolVersion": rpc.params["protocolVersion"].as_str().unwrap_or("2025-03-26"),
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "cuttar", "version": env!("CARGO_PKG_VERSION") }
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_defs() })),
            "tools/call" => {
                let name = rpc.params["name"].as_str().unwrap_or("");
                let args = rpc.params["arguments"].clone();
                let r = self.call(name, &args);
                self.audit.lock().push(AuditEntry { at: crate::model::now(), tool: name.into(), args: args.clone(), ok: r.is_ok() });
                Ok(match r {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": trim_for_model(&v) }] }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                })
            }
            _ => Err((-32601, format!("Method not found: {}", rpc.method))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, msg)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } }),
        })
    }

    fn call(&self, name: &str, a: &Value) -> Result<Value, String> {
        let (_, _, _, _, action) = TOOLS.iter().find(|t| t.0 == name).ok_or_else(|| format!("unknown tool {name}"))?;
        if *action == "*" {
            let act = a["action"].as_str().ok_or("action required")?;
            return self.lib.dispatch(act, a["args"].clone());
        }
        self.lib.dispatch(action, a.clone())
    }
}

/// Transcripts carry word timings the model does not need to read; the snapshot
/// is trimmed to what fits a context.
fn trim_for_model(v: &Value) -> String {
    let mut v = v.clone();
    if let Some(t) = v.get_mut("transcript") {
        if let Some(segs) = t.get_mut("segments").and_then(|s| s.as_array_mut()) {
            for s in segs.iter_mut() {
                if let Some(o) = s.as_object_mut() {
                    o.remove("words");
                }
            }
        }
    }
    if let Some(jobs) = v.get_mut("jobs").and_then(|j| j.as_array_mut()) {
        for j in jobs.iter_mut() {
            if let Some(o) = j.as_object_mut() {
                o.remove("log");
            }
        }
    }
    serde_json::to_string(&v).unwrap_or_default()
}

fn tool_defs() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|(name, desc, props, req, _)| {
            let required: Vec<&str> = req.split(',').filter(|s| !s.is_empty()).collect();
            json!({ "name": name, "description": desc, "inputSchema": { "type": "object", "properties": serde_json::from_str::<Value>(props).unwrap_or(json!({})), "required": required } })
        })
        .collect()
}

// ---- HTTP transport ----

fn header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}

fn respond_json(status: u16, body: String) -> Response<Cursor<Vec<u8>>> {
    let mut r = Response::from_string(body).with_status_code(status);
    r.add_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    cors(r)
}

/// The Vite dev page (a loopback origin) may call the API from a browser; the
/// origin gate below still refuses anything that is not loopback.
fn cors<R: std::io::Read>(mut r: Response<R>) -> Response<R> {
    r.add_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
    r.add_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Authorization, Content-Type"[..]).unwrap());
    r.add_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap());
    r
}

fn query_param(url: &str, key: &str) -> Option<String> {
    url.split('?').nth(1)?.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(|v| percent_decode(v)))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 1 && i + 2 <= b.len() - 1 + 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..(i + 3).min(s.len())], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn handle(server: &Arc<McpServer>, mut req: Request) {
    let path = req.url().split('?').next().unwrap_or("").to_string();
    if req.method() == &Method::Get && path == "/health" {
        let _ = req.respond(cors(Response::from_string("ok")));
        return;
    }
    if req.method() == &Method::Options {
        let _ = req.respond(cors(Response::from_string("")).with_status_code(204));
        return;
    }
    // A media file for the browser-mode preview: token in the query, only files under the output or data folders.
    if req.method() == &Method::Get && path == "/file" {
        let url = req.url().to_string();
        let ok = query_param(&url, "token").map(|t| ct_eq(&t, &server.token)).unwrap_or(false);
        let Some(p) = query_param(&url, "path") else {
            let _ = req.respond(Response::from_string("path required").with_status_code(400));
            return;
        };
        let out_dir = server.lib.out_dir();
        let allowed = std::path::Path::new(&p).canonicalize().map(|c| c.starts_with(&out_dir) || c.starts_with(&server.lib.data_dir)).unwrap_or(false);
        if !ok || !allowed {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            return;
        }
        match std::fs::File::open(&p) {
            Ok(f) => {
                let mime = match std::path::Path::new(&p).extension().and_then(|e| e.to_str()) { Some("mp4") => "video/mp4", Some("jpg") | Some("jpeg") => "image/jpeg", Some("png") => "image/png", _ => "application/octet-stream" };
                let mut r = Response::from_file(f);
                r.add_header(Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap());
                let _ = req.respond(cors(r));
            }
            Err(_) => {
                let _ = req.respond(Response::from_string("not found").with_status_code(404));
            }
        }
        return;
    }
    if req.method() != &Method::Post || !(path == "/mcp" || path == "/dispatch") {
        let _ = req.respond(Response::from_string("not found").with_status_code(404));
        return;
    }
    if !origin_ok(header(&req, "Origin")) || !host_ok(header(&req, "Host")) {
        let _ = req.respond(respond_json(403, r#"{"error":"forbidden origin"}"#.into()));
        return;
    }
    let caller = header(&req, "Authorization").and_then(|v| v.strip_prefix("Bearer ").map(str::to_string)).unwrap_or_default();
    if !ct_eq(&caller, &server.token) {
        let _ = req.respond(respond_json(401, r#"{"error":"unauthorized"}"#.into()));
        return;
    }
    let mut body = String::new();
    if req.as_reader().read_to_string(&mut body).is_err() {
        let _ = req.respond(respond_json(400, r#"{"error":"unreadable body"}"#.into()));
        return;
    }
    if path == "/dispatch" {
        let v: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                let _ = req.respond(respond_json(400, json!({ "error": format!("bad json: {e}") }).to_string()));
                return;
            }
        };
        let action = v["action"].as_str().unwrap_or("").to_string();
        let out = match server.lib.dispatch(&action, v["args"].clone()) {
            Ok(r) => respond_json(200, json!({ "ok": true, "result": r }).to_string()),
            Err(e) => respond_json(200, json!({ "ok": false, "error": e }).to_string()),
        };
        let _ = req.respond(out);
        return;
    }
    let rpc: Rpc = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            let _ = req.respond(respond_json(200, json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("Parse error: {e}") } }).to_string()));
            return;
        }
    };
    match server.handle_rpc(&rpc) {
        Some(v) => {
            let _ = req.respond(respond_json(200, v.to_string()));
        }
        None => {
            let _ = req.respond(Response::from_string("").with_status_code(202));
        }
    }
}

fn loopback(hostport: &str) -> bool {
    let host = if let Some(rest) = hostport.strip_prefix('[') { rest.split(']').next().unwrap_or("") } else { hostport.split(':').next().unwrap_or("") };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}
fn origin_ok(o: Option<&str>) -> bool {
    match o {
        None | Some("null") => true,
        Some(o) => loopback(o.strip_prefix("http://").or_else(|| o.strip_prefix("https://")).unwrap_or(o)),
    }
}
fn host_ok(h: Option<&str>) -> bool {
    h.map(loopback).unwrap_or(true)
}
fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gates_and_tools() {
        assert!(ct_eq("tok", "tok") && !ct_eq("tok", "toK") && !ct_eq("a", "ab"));
        assert!(origin_ok(None) && origin_ok(Some("http://127.0.0.1:1")) && !origin_ok(Some("https://evil.example")));
        assert!(host_ok(Some("localhost:9")) && !host_ok(Some("evil.example:9")));
        let defs = tool_defs();
        assert_eq!(defs.len(), TOOLS.len());
        assert!(defs.iter().all(|d| d["inputSchema"]["properties"].is_object()));
    }
}
