//! `cuttar` — the same core from a terminal, for automation and for Claude.
//!
//!   cuttar add <url|file> [language=nl] [transcriptSource=captions]
//!   cuttar status                       sources and their stages
//!   cuttar <action> [k=v] [k:=<json>]   any dispatch action against the running app
//!   cuttar headless                     run the core without a window (launchd, servers)
//!   cuttar mcp                          stdio MCP bridge to the running app (claude mcp add cuttar -- cuttar mcp)
//!   cuttar mcp-url                      the HTTP MCP endpoint + `claude mcp add` line
//!
//! One process owns the database: actions go to the running app (or `headless`)
//! over its loopback HTTP; the CLI never opens cuttar.sqlite itself.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dev = std::env::var("CUTTAR_DEV").is_ok();
    let Some(cmd) = args.first().map(String::as_str) else {
        eprintln!("{}", USAGE);
        std::process::exit(2);
    };
    match cmd {
        "help" | "-h" | "--help" => println!("{USAGE}"),
        "headless" => headless(dev),
        "mcp" => mcp_bridge(dev),
        "mcp-url" => match std::fs::read_to_string(cuttar_core::default_data_dir(dev).join("mcp.json")) {
            Ok(s) => println!("{s}"),
            Err(_) => fail("Cuttar is not running (no mcp.json yet). Start the app or `cuttar headless`."),
        },
        "add" => {
            let target = args.get(1).unwrap_or_else(|| fail("cuttar add <url|file>"));
            let mut a = kv(&args[2..]);
            if target.starts_with("http") { a["url"] = json!(target) } else { a["path"] = json!(std::fs::canonicalize(target).map(|p| p.display().to_string()).unwrap_or(target.clone())) }
            print(dispatch(dev, "add_source", a));
        }
        "status" => {
            let snap = dispatch(dev, "snapshot", json!({}));
            for s in snap["sources"].as_array().into_iter().flatten() {
                let n = snap["candidates"].as_array().map(|c| c.iter().filter(|c| c["sourceId"] == s["id"]).count()).unwrap_or(0);
                let ok = snap["candidates"].as_array().map(|c| c.iter().filter(|c| c["sourceId"] == s["id"] && c["approved"] == true).count()).unwrap_or(0);
                println!("{}  {:<12} {:>3} reels {:>3} ok  {}  {}", s["id"].as_str().unwrap_or(""), s["stage"].as_str().unwrap_or(""), n, ok, s["language"].as_str().unwrap_or("--"), s["title"].as_str().unwrap_or(""));
            }
            let jobs = snap["jobs"].as_array().map(|j| j.iter().filter(|j| matches!(j["status"].as_str(), Some("queued") | Some("running"))).count()).unwrap_or(0);
            println!("{jobs} jobs in flight · data {}", snap["paths"]["dataDir"].as_str().unwrap_or(""));
        }
        action => print(dispatch(dev, action, kv(&args[1..]))),
    }
}

const USAGE: &str = "cuttar add <url|file> [language=nl] [transcriptSource=captions|whisper|both]
cuttar status
cuttar <action> [key=value] [key:=json]     e.g. cuttar approve ids:='[\"c-…\"]'   cuttar render sourceId=s-…
cuttar headless | mcp | mcp-url | help
CUTTAR_DEV=1 talks to the Dev identity; CUTTAR_PORT / CUTTAR_DATA_DIR override the defaults.";

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

fn print(v: Value) {
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

/// key=value → string, key:=<json> → parsed.
fn kv(args: &[String]) -> Value {
    let mut m = serde_json::Map::new();
    for a in args {
        if let Some((k, v)) = a.split_once(":=") {
            m.insert(k.into(), serde_json::from_str(v).unwrap_or_else(|_| fail(&format!("bad json for {k}"))));
        } else if let Some((k, v)) = a.split_once('=') {
            m.insert(k.into(), json!(v));
        } else {
            fail(&format!("expected key=value, got {a}"));
        }
    }
    Value::Object(m)
}

fn endpoint(dev: bool) -> (String, String) {
    let dir = cuttar_core::default_data_dir(dev);
    let token = std::fs::read_to_string(dir.join("mcp-token")).map(|t| t.trim().to_string()).unwrap_or_else(|_| fail("Cuttar is not running (no token yet). Start the app or `cuttar headless`."));
    (format!("http://127.0.0.1:{}", cuttar_core::default_port(dev)), token)
}

fn dispatch(dev: bool, action: &str, args: Value) -> Value {
    let (base, token) = endpoint(dev);
    let resp = ureq::post(&format!("{base}/dispatch"))
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(600))
        .send_json(json!({ "action": action, "args": args }))
        .unwrap_or_else(|e| fail(&format!("Cuttar is not reachable at {base}: {e}\nStart the app or `cuttar headless`.")));
    let v: Value = resp.into_json().unwrap_or_else(|e| fail(&format!("bad reply: {e}")));
    if v["ok"] == true { v["result"].clone() } else { fail(v["error"].as_str().unwrap_or("error")) }
}

/// The core without a window: same data dir, same port, same MCP. Ctrl-C to stop.
fn headless(dev: bool) {
    cuttar_core::tools::inherit_login_path();
    let dir = cuttar_core::default_data_dir(dev);
    let log = cuttar_core::applog::init(&dir, log::LevelFilter::Info);
    let lib = cuttar_core::Library::open(&dir, Some(cuttar_core::default_port(dev))).unwrap_or_else(|e| fail(&e));
    let mcp = lib.mcp.get().unwrap();
    println!("cuttar headless · data {} · log {} · MCP {}", dir.display(), log.display(), mcp.url());
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

/// stdin JSON-RPC lines → the app's /mcp → stdout. For clients that only speak stdio.
fn mcp_bridge(dev: bool) {
    let (base, token) = endpoint(dev);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let resp = ureq::post(&format!("{base}/mcp")).set("Authorization", &format!("Bearer {token}")).set("Content-Type", "application/json").timeout(std::time::Duration::from_secs(600)).send_string(&line);
        match resp {
            Ok(r) if r.status() == 200 => {
                let body = r.into_string().unwrap_or_default();
                let _ = writeln!(out, "{body}");
                let _ = out.flush();
            }
            Ok(_) => {} // 202: a notification, no reply
            Err(e) => {
                let _ = writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32000, "message": e.to_string() } }));
                let _ = out.flush();
            }
        }
    }
}
