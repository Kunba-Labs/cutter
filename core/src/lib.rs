//! `cuttar-core`: the library behind the desktop app, the CLI and the MCP.
//! One write entry point, [`Library::dispatch`], shared by all three.

pub mod applog;
pub mod brain;
pub mod captions;
pub mod db;
pub mod ffmpeg;
pub mod mcp;
pub mod model;
pub mod pipeline;
pub mod posters;
pub mod tools;
pub mod whisper;
pub mod youtube;
pub mod ytdlp;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use db::Db;
use model::*;

pub type Notify = Box<dyn Fn() + Send + Sync>;

pub const BUNDLE_ID: &str = "com.cuttar.desktop";

/// `CUTTAR_DATA_DIR`, else ~/Library/Application Support/com.cuttar.desktop[.dev].
pub fn default_data_dir(dev: bool) -> PathBuf {
    if let Ok(d) = std::env::var("CUTTAR_DATA_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Library/Application Support").join(if dev { format!("{BUNDLE_ID}.dev") } else { BUNDLE_ID.to_string() })
}

/// `CUTTAR_PORT`, else 47251 (prod) / 47252 (dev). Fixed, so `claude mcp add` stays valid.
pub fn default_port(dev: bool) -> u16 {
    std::env::var("CUTTAR_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(if dev { 47252 } else { 47251 })
}

pub struct Library {
    pub db: Mutex<Db>,
    pub data_dir: PathBuf,
    pub mcp: OnceLock<Arc<mcp::McpServer>>,
    notify: Mutex<Option<Notify>>,
    cancel: Mutex<HashSet<String>>,
    children: Mutex<HashMap<String, u32>>,
    active: Mutex<Vec<String>>,
    /// Set by `shutdown`: no job is claimed any more; the process exits once the active ones finish.
    draining: std::sync::atomic::AtomicBool,
}

impl Library {
    /// Open the library, restart what was mid-flight, start the workers and
    /// the scheduler. `port` is where the MCP/HTTP server listens; None = none.
    pub fn open(data_dir: &Path, port: Option<u16>) -> Result<Arc<Library>, String> {
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        // The app launches with cwd "/". Children (claude, uv, whisper) scan their cwd, which from "/"
        // reaches Downloads, Photos, Music and network volumes and makes macOS ask for each.
        let _ = std::env::set_current_dir(data_dir);
        let db = Db::open(&data_dir.join("cuttar.sqlite")).map_err(|e| e.to_string())?;
        let lib = Arc::new(Library {
            db: Mutex::new(db),
            data_dir: data_dir.to_path_buf(),
            mcp: OnceLock::new(),
            notify: Mutex::new(None),
            cancel: Mutex::new(HashSet::new()),
            children: Mutex::new(HashMap::new()),
            active: Mutex::new(Vec::new()),
            draining: std::sync::atomic::AtomicBool::new(false),
        });
        std::fs::create_dir_all(lib.out_dir()).map_err(|e| e.to_string())?;
        posters::qr_script(data_dir);
        // A job that was running when the app quit goes back to the queue.
        for mut j in lib.all::<Job>("jobs") {
            if j.status == "running" {
                j.status = "queued".into();
                j.message = "restarted".into();
                lib.put("jobs", &j.id.clone(), &j.created_at.clone(), &j);
            }
        }
        if let Some(p) = port {
            let s = mcp::McpServer::spawn(lib.clone(), p)?;
            let _ = lib.mcp.set(s);
        }
        let n = lib.settings().parallel_jobs.clamp(1, 4);
        for i in 0..n {
            let w = lib.clone();
            std::thread::Builder::new().name(format!("cuttar-worker-{i}")).spawn(move || pipeline::worker(w)).map_err(|e| e.to_string())?;
        }
        let s = lib.clone();
        std::thread::Builder::new().name("cuttar-scheduler".into()).spawn(move || pipeline::scheduler(s)).map_err(|e| e.to_string())?;
        lib.migrate_targets();
        Ok(lib)
    }

    pub fn on_change(&self, f: Notify) {
        *self.notify.lock() = Some(f);
    }

    pub fn changed(&self) {
        if let Some(f) = self.notify.lock().as_ref() {
            f();
        }
    }

    pub fn settings(&self) -> Settings {
        let mut st = self.db.lock().settings();
        // Templates saved before the title fields existed carry the plain defaults;
        // an untouched built-in takes its title look from the current definition.
        let plain = CaptionStyle::default();
        for t in st.caption_templates.iter_mut() {
            let untouched = t.hook_color == plain.hook_color && t.hook_box_color == plain.hook_box_color && t.hook_size == plain.hook_size && t.hook_font == plain.hook_font && !t.hook_uppercase && !t.hook_italic && t.hook_boxed == plain.hook_boxed;
            if let Some(b) = untouched.then(|| model::caption_templates().into_iter().find(|b| b.name == t.name)).flatten() {
                t.hook_font = b.hook_font;
                t.hook_size = b.hook_size;
                t.hook_color = b.hook_color;
                t.hook_boxed = b.hook_boxed;
                t.hook_box_color = b.hook_box_color;
                t.hook_uppercase = b.hook_uppercase;
                t.hook_bold = b.hook_bold;
                t.hook_italic = b.hook_italic;
                t.hook_pct = b.hook_pct;
            }
        }
        st
    }

    pub fn out_dir(&self) -> PathBuf {
        PathBuf::from(self.settings().out_dir)
    }

    // ---- records ----

    pub fn get<T: DeserializeOwned>(&self, table: &str, id: &str) -> Option<T> {
        self.db.lock().get(table, id).ok().flatten()
    }

    pub fn all<T: DeserializeOwned>(&self, table: &str) -> Vec<T> {
        self.db.lock().all(table).unwrap_or_default()
    }

    pub fn put<T: Serialize>(&self, table: &str, id: &str, ord: &str, v: &T) {
        if let Err(e) = self.db.lock().put(table, id, ord, v) {
            log::error!("put {table}/{id}: {e}");
        }
    }

    pub fn save_source(&self, s: &mut Source) {
        s.updated_at = now();
        self.put("sources", &s.id.clone(), &s.added_at.clone(), s);
        self.changed();
    }

    pub fn save_candidate(&self, c: &mut Candidate) {
        c.updated_at = now();
        self.put("candidates", &c.id.clone(), &format!("{}-{:08.2}", c.source_id, c.start), c);
        self.changed();
    }

    pub fn save_post(&self, p: &mut Post) {
        p.updated_at = now();
        self.put("posts", &p.id.clone(), &p.scheduled_at.clone(), p);
        self.changed();
    }

    pub fn save_target(&self, t: &mut Target) {
        if t.created_at.is_empty() {
            t.created_at = now();
        }
        self.put("targets", &t.id.clone(), &t.created_at.clone(), t);
        self.changed();
    }

    /// Enabled targets, oldest first.
    pub fn targets(&self) -> Vec<Target> {
        self.all::<Target>("targets").into_iter().filter(|t| t.enabled).collect()
    }

    /// The target a post goes to: its own, else the first enabled one of its kind.
    pub fn target_for(&self, p: &Post) -> Option<Target> {
        if !p.target_id.is_empty() {
            if let Some(t) = self.get::<Target>("targets", &p.target_id) {
                return Some(t);
            }
        }
        self.targets().into_iter().find(|t| t.kind == p.channel)
    }

    /// The YouTube login of a target, with the shared OAuth client.
    pub fn youtube_auth(&self, t: &Target) -> YoutubeAuth {
        let base = self.settings().youtube;
        YoutubeAuth { refresh_token: t.refresh_token.clone(), channel_title: t.channel_title.clone(), privacy: t.privacy.clone(), category_id: t.category_id.clone(), title_suffix: t.title_suffix.clone(), description_footer: t.description_footer.clone(), ..base }
    }

    /// One-time: the single YouTube login in settings becomes the first target; posts get its id.
    fn migrate_targets(&self) {
        if !self.all::<Target>("targets").is_empty() {
            return;
        }
        let s = self.settings();
        if s.youtube.refresh_token.is_empty() {
            return;
        }
        let hours = s.cadence["youtube"].as_array().map(|h| h.iter().filter_map(|v| v.as_i64()).collect()).unwrap_or_default();
        let mut t = Target { id: new_id("t"), kind: "youtube".into(), name: if s.youtube.channel_title.is_empty() { "YouTube".into() } else { s.youtube.channel_title.clone() }, hours, refresh_token: s.youtube.refresh_token.clone(), channel_title: s.youtube.channel_title.clone(), privacy: s.youtube.privacy.clone(), category_id: s.youtube.category_id.clone(), title_suffix: s.youtube.title_suffix.clone(), description_footer: s.youtube.description_footer.clone(), ..Default::default() };
        self.save_target(&mut t);
        // Posts without a target, or pointing at one that no longer exists, go to this one.
        let known: Vec<String> = self.all::<Target>("targets").into_iter().map(|t| t.id).collect();
        for mut p in self.all::<Post>("posts").into_iter().filter(|p| p.channel == "youtube" && (p.target_id.is_empty() || !known.contains(&p.target_id))) {
            p.target_id = t.id.clone();
            self.save_post(&mut p);
        }
    }

    pub fn save_poster(&self, p: &mut Poster) {
        p.updated_at = now();
        self.put("posters", &p.id.clone(), &p.created_at.clone(), p);
        self.changed();
    }

    pub fn save_settings(&self, s: &Settings) {
        let _ = self.db.lock().save_settings(s);
        self.changed();
    }

    // ---- jobs ----

    /// Queue a job unless the same kind+ref is already queued or running.
    pub fn enqueue(&self, kind: &str, ref_id: &str, label: &str, args: Value) -> String {
        if let Some(j) = self.all::<Job>("jobs").into_iter().find(|j| j.kind == kind && j.ref_id == ref_id && matches!(j.status.as_str(), "queued" | "running") && j.args == args) {
            return j.id;
        }
        let j = Job { id: new_id("j"), kind: kind.into(), ref_id: ref_id.into(), label: label.into(), status: "queued".into(), args, created_at: now(), ..Default::default() };
        self.put("jobs", &j.id, &j.created_at, &j);
        self.changed();
        j.id
    }

    /// The oldest queued job this worker may take: one transcript at a time
    /// (the GPU), everything else in parallel. ponytail: a single lock, a scan.
    pub fn claim_job(&self) -> Option<Job> {
        if self.draining.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let db = self.db.lock();
        let jobs: Vec<Job> = db.all("jobs").unwrap_or_default();
        let transcribing = jobs.iter().any(|j| j.status == "running" && j.kind == "transcribe");
        let mut j = jobs.into_iter().find(|j| j.status == "queued" && !(transcribing && j.kind == "transcribe"))?;
        j.status = "running".into();
        j.started_at = Some(now());
        let _ = db.put("jobs", &j.id, &j.created_at, &j);
        drop(db);
        self.active.lock().push(j.id.clone());
        self.changed();
        Some(j)
    }

    pub fn finish_job(&self, id: &str, result: Result<Value, String>) {
        if let Some(mut j) = self.get::<Job>("jobs", id) {
            let cancelled = self.cancel.lock().remove(id);
            match result {
                Ok(v) => {
                    j.status = "done".into();
                    j.progress = 1.0;
                    if let Some(m) = v["message"].as_str() {
                        j.message = m.into();
                    }
                }
                Err(e) if cancelled => {
                    j.status = "cancelled".into();
                    j.message = "cancelled".into();
                    let _ = e;
                }
                Err(e) => {
                    j.status = "failed".into();
                    j.message = e;
                }
            }
            j.finished_at = Some(now());
            self.put("jobs", &j.id.clone(), &j.created_at.clone(), &j);
        }
        self.children.lock().remove(id);
        self.active.lock().retain(|a| a != id);
        self.changed();
    }

    pub fn job_progress(&self, id: &str, p: f64, msg: &str) {
        if let Some(mut j) = self.get::<Job>("jobs", id) {
            j.progress = p;
            j.message = msg.into();
            self.put("jobs", &j.id.clone(), &j.created_at.clone(), &j);
            self.changed();
        }
    }

    pub fn job_log(&self, id: &str, line: &str) {
        if let Some(mut j) = self.get::<Job>("jobs", id) {
            j.log.push_str(line);
            j.log.push('\n');
            if j.log.len() > 12_000 {
                j.log = j.log[j.log.len() - 12_000..].to_string();
            }
            self.put("jobs", &j.id.clone(), &j.created_at.clone(), &j);
        }
        log::info!("[{id}] {line}");
    }

    pub fn register_child(&self, job_id: &str, pid: u32) {
        self.children.lock().insert(job_id.to_string(), pid);
    }

    pub fn cancelled(&self, job_id: &str) -> bool {
        self.cancel.lock().contains(job_id)
    }

    fn cancel_job(&self, id: &str) {
        self.cancel.lock().insert(id.to_string());
        if let Some(pid) = self.children.lock().get(id).copied() {
            let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
        }
        if let Some(mut j) = self.get::<Job>("jobs", id) {
            if j.status == "queued" {
                j.status = "cancelled".into();
                j.finished_at = Some(now());
                self.put("jobs", &j.id.clone(), &j.created_at.clone(), &j);
                self.cancel.lock().remove(id);
                self.changed();
            }
        }
    }

    // ---- the shared surface ----

    pub fn snapshot(&self) -> Value {
        let s = self.settings();
        let mut jobs: Vec<Job> = self.all("jobs");
        if jobs.len() > 300 {
            jobs = jobs.split_off(jobs.len() - 300);
        }
        json!({
            "sources": self.all::<Source>("sources"),
            "channels": self.all::<Channel>("channels"),
            "targets": self.all::<Target>("targets"),
            "inbox": self.all::<InboxItem>("inbox"),
            "candidates": self.all::<Candidate>("candidates"),
            "renders": self.all::<Render>("renders"),
            "posts": self.all::<Post>("posts"),
            "posters": self.all::<Poster>("posters"),
            "jobs": jobs,
            "settings": s,
            "tools": tools::detect(&s.claude_bin),
            "mcp": self.mcp.get().map(|m| json!({ "url": m.url(), "token": m.token })),
            "paths": { "dataDir": self.data_dir, "outDir": s.out_dir, "log": self.data_dir.join("cuttar.log") },
        })
    }

    pub fn add_source(&self, a: &Value) -> Result<Source, String> {
        let s = self.settings();
        let url = a["url"].as_str().map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
        let path = a["path"].as_str().map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        let (kind, title, meta) = if let Some(u) = &url {
            if !u.starts_with("http") {
                return Err("That is not a URL".into());
            }
            if let Some(existing) = self.all::<Source>("sources").into_iter().find(|s| s.url.as_deref() == Some(u)) {
                return Err(format!("Already in the library: {}", existing.title));
            }
            (if a["channelId"].is_string() { "channel" } else { "youtube" }, u.clone(), Value::Null)
        } else if let Some(p) = &path {
            if !Path::new(p).exists() {
                return Err(format!("No such file: {p}"));
            }
            ("file", Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), Value::Null)
        } else {
            return Err("url or path required".into());
        };
        let id = new_id("s");
        let mut src = Source {
            id: id.clone(),
            kind: kind.into(),
            url,
            path,
            title: a["title"].as_str().map(String::from).unwrap_or(title),
            channel_id: a["channelId"].as_str().map(String::from),
            lang_override: a["language"].as_str().filter(|l| !l.is_empty() && *l != "auto").map(String::from),
            stage: "added".into(),
            transcript_source: a["transcriptSource"].as_str().unwrap_or("whisper").into(),
            auto_detect: a["autoDetect"].as_bool().unwrap_or(s.auto_detect),
            auto_approve_score: a["autoApproveScore"].as_i64().unwrap_or(s.auto_approve_score),
            auto_schedule: a["autoSchedule"].as_bool().unwrap_or(false),
            meta,
            added_at: now(),
            ..Default::default()
        };
        src.folder = self.out_dir().join(format!("{}-{}", slug(&src.title), &id[2..8])).display().to_string();
        self.save_source(&mut src);
        self.enqueue("download", &id, &src.title, json!({}));
        Ok(src)
    }

    pub fn dispatch(&self, action: &str, a: Value) -> Result<Value, String> {
        let s = |k: &str| a[k].as_str().map(str::to_string);
        let id = || s("id").ok_or_else(|| "id required".to_string());
        let out = match action {
            "snapshot" => self.snapshot(),
            "source" => {
                let id = id()?;
                let src: Source = self.get("sources", &id).ok_or("no such source")?;
                let transcript: Option<Transcript> = self.get("transcripts", &id);
                let candidates: Vec<Candidate> = self.all::<Candidate>("candidates").into_iter().filter(|c| c.source_id == id).collect();
                let renders: Vec<Render> = self.all::<Render>("renders").into_iter().filter(|r| r.source_id == id).collect();
                json!({ "source": src, "transcript": transcript, "candidates": candidates, "renders": renders })
            }
            "add_source" => json!(self.add_source(&a)?),
            "remove_source" => {
                let id = id()?;
                let db = self.db.lock();
                let _ = db.delete("sources", &id);
                let _ = db.delete("transcripts", &id);
                let _ = db.delete_where::<Candidate>("candidates", |c| c.source_id == id);
                let _ = db.delete_where::<Render>("renders", |r| r.source_id == id);
                let _ = db.conn.execute("DELETE FROM transcripts_fts WHERE source_id=?1", rusqlite::params![id]);
                drop(db);
                self.changed();
                json!(true)
            }
            "run" => {
                let id = id()?;
                let src: Source = self.get("sources", &id).ok_or("no such source")?;
                let stage = s("stage").unwrap_or_else(|| match src.stage.as_str() {
                    "added" | "failed" if src.video_path.is_none() => "download".into(),
                    "downloaded" => "transcribe".into(),
                    _ => "detect".into(),
                });
                json!(self.enqueue(&stage, &id, &src.title, json!({})))
            }
            "pick_cover" => {
                // Forget the chosen frame and render the reel's formats again: the next render picks afresh.
                let id = id()?;
                let mut c: Candidate = self.get("candidates", &id).ok_or("no such reel")?;
                c.cover_t = None;
                self.save_candidate(&mut c);
                self.dispatch("render", json!({ "candidateIds": [id] }))?
            }
            "verify_renders" => {
                // Measure finished renders again (sourceId, candidateIds, or all): fills info + issues.
                let mut ids: Vec<String> = a["candidateIds"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                if let Some(sid) = s("sourceId") {
                    ids.extend(self.all::<Candidate>("candidates").into_iter().filter(|c| c.source_id == sid).map(|c| c.id));
                }
                let all = ids.is_empty();
                let st = self.settings();
                let mut n = 0;
                let mut flagged = 0;
                for mut r in self.all::<Render>("renders").into_iter().filter(|r| r.status == "done" && (all || ids.contains(&r.candidate_id))) {
                    let Some(c) = self.get::<Candidate>("candidates", &r.candidate_id) else { continue };
                    let (w, h, _) = ffmpeg::format_spec(&r.format);
                    let src = self.get::<Source>("sources", &r.source_id);
                    let want_audio = src.as_ref().map(|s| s.meta["hasAudio"].as_bool().unwrap_or(true)).unwrap_or(true);
                    let (start, end) = ((c.start - 0.25).max(0.0), (c.end + 0.35).min(src.as_ref().and_then(|s| s.duration).unwrap_or(f64::MAX)));
                    let want = (end - start) + if st.end_card.enabled && st.end_card.paths[ffmpeg::aspect_key(w, h)].as_str().map(|p| Path::new(p).exists()).unwrap_or(false) { st.end_card.seconds.clamp(0.5, 8.0) } else { 0.0 };
                    match ffmpeg::probe(Path::new(&r.path)) {
                        Ok(pr) => {
                            let issues = ffmpeg::quality_issues(&pr, w, h, want, want_audio);
                            if !issues.is_empty() {
                                flagged += 1;
                            }
                            r.info = ffmpeg::info_json(&pr, Path::new(&r.path));
                            r.info["issues"] = json!(issues);
                        }
                        Err(e) => {
                            flagged += 1;
                            r.info = json!({ "issues": [format!("unreadable: {e}")] });
                        }
                    }
                    self.put("renders", &r.id.clone(), &r.created_at.clone(), &r);
                    n += 1;
                }
                // Sources too: the facts a download would have stored.
                let mut sources = 0;
                for mut src in self.all::<Source>("sources").into_iter().filter(|x| x.video_path.is_some() && (all || s("sourceId").as_deref() == Some(x.id.as_str()))) {
                    if let Ok(pr) = ffmpeg::probe(Path::new(src.video_path.as_ref().unwrap())) {
                        src.meta["width"] = json!(pr.width);
                        src.meta["height"] = json!(pr.height);
                        src.meta["fps"] = json!(pr.fps);
                        src.meta["vcodec"] = json!(pr.vcodec);
                        src.meta["kbps"] = json!(pr.kbps);
                        src.meta["hasAudio"] = json!(pr.has_audio);
                        let mut issues: Vec<String> = Vec::new();
                        if pr.width < 640 || pr.height < 360 {
                            issues.push(format!("only {}×{}", pr.width, pr.height));
                        }
                        if pr.duration < 1.0 {
                            issues.push("no length".into());
                        }
                        if !pr.has_audio {
                            issues.push("no audio".into());
                        }
                        src.meta["issues"] = json!(issues);
                        self.save_source(&mut src);
                        sources += 1;
                    }
                }
                self.changed();
                json!({ "checked": n, "flagged": flagged, "sources": sources })
            }
            "covers" => {
                // Cover images again for every finished render of these reels (title or frame changed). No re-encode.
                let mut ids: Vec<String> = a["candidateIds"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                if let Some(sid) = s("sourceId") {
                    ids.extend(self.all::<Candidate>("candidates").into_iter().filter(|c| c.source_id == sid).map(|c| c.id));
                }
                let mut n = 0;
                for r in self.all::<Render>("renders").into_iter().filter(|r| r.status == "done" && ids.contains(&r.candidate_id)) {
                    let Some(c) = self.get::<Candidate>("candidates", &r.candidate_id) else { continue };
                    match pipeline::make_cover(self, &c, &r.format) {
                        Ok(p) => {
                            let mut r = r;
                            r.cover_path = Some(p.display().to_string());
                            self.put("renders", &r.id.clone(), &r.created_at.clone(), &r);
                            n += 1;
                        }
                        Err(e) => log::warn!("cover {} {}: {e}", c.id, r.format),
                    }
                }
                self.changed();
                json!(n)
            }
            "redownload" => {
                // The best stream again; transcript, reels and edits stay.
                let id = id()?;
                let src: Source = self.get("sources", &id).ok_or("no such source")?;
                if src.url.is_none() && src.path.is_none() {
                    return Err("nothing to fetch again".into());
                }
                json!(self.enqueue("download", &id, &src.title, json!({ "keep": true })))
            }
            "edit_segment" => {
                // A corrected line of the transcript. Same word count keeps every timing;
                // otherwise the new words share the line's span evenly.
                let id = id()?;
                let idx = a["index"].as_u64().ok_or("index required")? as usize;
                let text = s("text").ok_or("text required")?.trim().to_string();
                let mut t: Transcript = self.get("transcripts", &id).ok_or("no transcript")?;
                let seg = t.segments.get_mut(idx).ok_or("no such line")?;
                let new_words: Vec<&str> = text.split_whitespace().collect();
                if new_words.is_empty() {
                    return Err("a line cannot be empty".into());
                }
                if seg.words.len() == new_words.len() {
                    for (w, nw) in seg.words.iter_mut().zip(new_words.iter()) {
                        w.w = nw.to_string();
                    }
                } else {
                    let (a0, b0) = (seg.start, seg.end);
                    let n = new_words.len() as f64;
                    seg.words = new_words.iter().enumerate().map(|(i, w)| Word { w: w.to_string(), s: a0 + (b0 - a0) * i as f64 / n, e: a0 + (b0 - a0) * (i as f64 + 1.0) / n }).collect();
                }
                seg.text = text;
                let full: String = t.segments.iter().map(|g| g.text.as_str()).collect::<Vec<_>>().join(" ");
                self.put("transcripts", &id, &now(), &t);
                let _ = self.db.lock().fts_put(&id, &full);
                if let Some(src) = self.get::<Source>("sources", &id) {
                    let _ = std::fs::write(Path::new(&src.folder).join("transcript.srt"), captions::srt(&t.segments, 0.0, f64::MAX));
                }
                self.changed();
                json!(t.segments[idx])
            }
            "edit_word" => {
                // One word of a line, timing untouched. `word` is its index within the line.
                let id = id()?;
                let idx = a["index"].as_u64().ok_or("index required")? as usize;
                let wi = a["word"].as_u64().ok_or("word required")? as usize;
                // Several words are fine ("Allah SWT"): they share the old word's time span evenly. Empty drops the word.
                let new: Vec<String> = s("text").unwrap_or_default().split_whitespace().map(String::from).collect();
                let mut t: Transcript = self.get("transcripts", &id).ok_or("no transcript")?;
                let seg = t.segments.get_mut(idx).ok_or("no such line")?;
                let mut tokens: Vec<String> = seg.text.split_whitespace().map(String::from).collect();
                if seg.words.is_empty() {
                    if wi >= tokens.len() {
                        return Err("no such word".into());
                    }
                    tokens.splice(wi..wi + 1, new);
                } else {
                    let old = seg.words.get(wi).ok_or("no such word")?.clone();
                    let n = new.len() as f64;
                    let timed: Vec<Word> = new.iter().enumerate().map(|(i, w)| Word { w: w.clone(), s: old.s + (old.e - old.s) * i as f64 / n, e: old.s + (old.e - old.s) * (i + 1) as f64 / n }).collect();
                    seg.words.splice(wi..wi + 1, timed);
                    tokens = seg.words.iter().map(|w| w.w.clone()).collect();
                }
                seg.text = tokens.join(" ");
                let full: String = t.segments.iter().map(|g| g.text.as_str()).collect::<Vec<_>>().join(" ");
                self.put("transcripts", &id, &now(), &t);
                let _ = self.db.lock().fts_put(&id, &full);
                if let Some(src) = self.get::<Source>("sources", &id) {
                    let _ = std::fs::write(Path::new(&src.folder).join("transcript.srt"), captions::srt(&t.segments, 0.0, f64::MAX));
                }
                self.changed();
                json!(t.segments[idx])
            }
            "polish" => {
                let id = id()?;
                let src: Source = self.get("sources", &id).ok_or("no such source")?;
                json!(self.enqueue("polish", &id, &src.title, json!({ "thenDetect": a["thenDetect"].as_bool().unwrap_or(false) })))
            }
            "set_language" => {
                let id = id()?;
                let mut src: Source = self.get("sources", &id).ok_or("no such source")?;
                let lang = s("language").unwrap_or_default();
                src.lang_override = if lang.is_empty() || lang == "auto" { None } else { Some(lang) };
                self.save_source(&mut src);
                if a["retranscribe"].as_bool().unwrap_or(true) && src.video_path.is_some() {
                    self.enqueue("transcribe", &id, &src.title, json!({}));
                }
                json!(src)
            }
            "update_source" => {
                let id = id()?;
                let mut src: Source = self.get("sources", &id).ok_or("no such source")?;
                let mut v = serde_json::to_value(&src).unwrap();
                merge(&mut v, &a["patch"]);
                src = serde_json::from_value(v).map_err(|e| e.to_string())?;
                self.save_source(&mut src);
                json!(src)
            }
            "update_candidate" => {
                let id = id()?;
                let mut c: Candidate = self.get("candidates", &id).ok_or("no such candidate")?;
                let mut v = serde_json::to_value(&c).unwrap();
                merge(&mut v, &a["patch"]);
                c = serde_json::from_value(v).map_err(|e| e.to_string())?;
                if c.end - c.start > self.settings().max_reel_s as f64 + 0.05 {
                    return Err(format!("A reel is at most {} s", self.settings().max_reel_s));
                }
                self.save_candidate(&mut c);
                json!(c)
            }
            "apply_look" => {
                // Copy this reel's adjustments onto the others: keys = style, hookStyle, captionPct, crop, formats;
                // scope = source (same lecture) | approved | library.
                let id = id()?;
                let from: Candidate = self.get("candidates", &id).ok_or("no such candidate")?;
                let keys: Vec<String> = a["keys"].as_array().map(|k| k.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["style".into(), "hookStyle".into(), "captionPct".into()]);
                let scope = s("scope").unwrap_or_else(|| "source".into());
                let mut n = 0;
                for mut c in self.all::<Candidate>("candidates") {
                    if c.id == from.id || c.discarded {
                        continue;
                    }
                    let in_scope = match scope.as_str() {
                        "approved" => c.approved,
                        "library" => true,
                        _ => c.source_id == from.source_id,
                    };
                    if !in_scope {
                        continue;
                    }
                    for k in &keys {
                        match k.as_str() {
                            "style" => c.style = from.style.clone(),
                            "hookStyle" => c.hook_style = from.hook_style.clone(),
                            "captionPct" => c.caption_pct = from.caption_pct,
                            "crop" => c.crop = from.crop.clone(),
                            "formats" => c.formats = from.formats.clone(),
                            _ => {}
                        }
                    }
                    self.save_candidate(&mut c);
                    n += 1;
                }
                json!(n)
            }
            "approve" => {
                let ids: Vec<String> = a["ids"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_else(|| s("id").into_iter().collect());
                let approved = a["approved"].as_bool().unwrap_or(true);
                for id in &ids {
                    if let Some(mut c) = self.get::<Candidate>("candidates", id) {
                        c.approved = approved;
                        c.discarded = false;
                        self.save_candidate(&mut c);
                    }
                }
                json!(ids.len())
            }
            "approve_above" => {
                let sid = s("sourceId").ok_or("sourceId required")?;
                let min = a["score"].as_i64().unwrap_or(8);
                let mut n = 0;
                for mut c in self.all::<Candidate>("candidates").into_iter().filter(|c| c.source_id == sid && c.score >= min && !c.discarded) {
                    c.approved = true;
                    self.save_candidate(&mut c);
                    n += 1;
                }
                json!(n)
            }
            "discard" => {
                let id = id()?;
                let mut c: Candidate = self.get("candidates", &id).ok_or("no such candidate")?;
                c.discarded = !c.discarded;
                c.approved = false;
                self.save_candidate(&mut c);
                json!(c)
            }
            "add_candidate" => {
                let sid = s("sourceId").ok_or("sourceId required")?;
                let (start, end) = (a["start"].as_f64().ok_or("start")?, a["end"].as_f64().ok_or("end")?);
                let mut c = Candidate { id: new_id("c"), source_id: sid, start, end: end.min(start + self.settings().max_reel_s as f64), category: "statement".into(), score: 7, title: s("title").unwrap_or_else(|| "Handpicked".into()), crop: json!({ "x": 0.5 }), created_at: now(), ..Default::default() };
                self.save_candidate(&mut c);
                json!(c)
            }
            "render" => {
                let st = self.settings();
                let mut ids: Vec<String> = a["candidateIds"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                if let Some(sid) = s("sourceId") {
                    ids.extend(self.all::<Candidate>("candidates").into_iter().filter(|c| c.source_id == sid && c.approved && !c.discarded).map(|c| c.id));
                }
                let formats_arg: Option<Vec<String>> = a["formats"].as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect());
                let mut jobs = Vec::new();
                for cid in ids {
                    let Some(c) = self.get::<Candidate>("candidates", &cid) else { continue };
                    let formats = formats_arg.clone().unwrap_or(if c.formats.is_empty() { st.formats.clone() } else { c.formats.clone() });
                    for f in formats {
                        jobs.push(self.enqueue("render", &cid, &format!("{} · {f}", c.title), json!({ "format": f })));
                    }
                }
                json!(jobs)
            }
            "jobs" => json!(self.all::<Job>("jobs")),
            "cancel_job" => {
                self.cancel_job(&id()?);
                json!(true)
            }
            "retry_job" => {
                let id = id()?;
                let j: Job = self.get("jobs", &id).ok_or("no such job")?;
                json!(self.enqueue(&j.kind, &j.ref_id, &j.label, j.args.clone()))
            }
            "remove_job" => {
                // Drops one job, failed or not. A live one is cancelled first.
                let id = id()?;
                if let Some(j) = self.get::<Job>("jobs", &id) {
                    if matches!(j.status.as_str(), "running" | "queued") {
                        self.cancel_job(&id);
                    }
                }
                let _ = self.db.lock().delete("jobs", &id);
                self.changed();
                json!(true)
            }
            "clear_jobs" => {
                let n = self.db.lock().delete_where::<Job>("jobs", |j| matches!(j.status.as_str(), "done" | "cancelled" | "failed")).unwrap_or(0);
                self.changed();
                json!(n)
            }
            "add_channel" => {
                let url = s("url").ok_or("url required")?;
                let mut c = Channel { id: new_id("ch"), url: url.clone(), name: s("name").unwrap_or(url), interval_h: a["intervalH"].as_i64().unwrap_or(6), min_len_s: a["minLenS"].as_i64().unwrap_or(600), title_regex: s("titleRegex").unwrap_or_default(), since: s("since").unwrap_or_default(), auto_process: a["autoProcess"].as_bool().unwrap_or(true), auto_approve_score: a["autoApproveScore"].as_i64().unwrap_or(0), auto_schedule: false, enabled: true, ..Default::default() };
                let mut v = serde_json::to_value(&c).unwrap();
                merge(&mut v, &a);
                v["id"] = json!(c.id);
                c = serde_json::from_value(v).map_err(|e| e.to_string())?;
                self.put("channels", &c.id, &now(), &c);
                self.enqueue("check_channel", &c.id, &c.name, json!({}));
                self.changed();
                json!(c)
            }
            "update_channel" => {
                let id = id()?;
                let mut c: Channel = self.get("channels", &id).ok_or("no such channel")?;
                let mut v = serde_json::to_value(&c).unwrap();
                merge(&mut v, &a["patch"]);
                c = serde_json::from_value(v).map_err(|e| e.to_string())?;
                self.put("channels", &c.id, &now(), &c);
                self.changed();
                json!(c)
            }
            "remove_channel" => {
                let id = id()?;
                let db = self.db.lock();
                let _ = db.delete("channels", &id);
                let _ = db.delete_where::<InboxItem>("inbox", |i| i.channel_id == id);
                drop(db);
                self.changed();
                json!(true)
            }
            "check_channel" => {
                let ids: Vec<Channel> = match s("id") {
                    Some(id) => self.get::<Channel>("channels", &id).into_iter().collect(),
                    None => self.all("channels"),
                };
                json!(ids.iter().map(|c| self.enqueue("check_channel", &c.id, &c.name, json!({}))).collect::<Vec<_>>())
            }
            "process_inbox" => {
                let id = id()?;
                let mut item: InboxItem = self.get("inbox", &id).ok_or("no such inbox item")?;
                let ch: Option<Channel> = self.get("channels", &item.channel_id);
                let src = self.add_source(&json!({ "url": item.url, "title": item.title, "channelId": item.channel_id, "autoApproveScore": ch.as_ref().map(|c| c.auto_approve_score).unwrap_or(0), "autoSchedule": ch.as_ref().map(|c| c.auto_schedule).unwrap_or(false) }))?;
                item.status = "processed".into();
                item.source_id = Some(src.id.clone());
                self.put("inbox", &item.id, &item.found_at, &item);
                self.changed();
                json!(src)
            }
            "skip_inbox" => {
                let id = id()?;
                let mut item: InboxItem = self.get("inbox", &id).ok_or("no such inbox item")?;
                item.status = "skipped".into();
                self.put("inbox", &item.id, &item.found_at, &item);
                self.changed();
                json!(item)
            }
            "schedule" => {
                // targetId picks the target; channel alone means the first enabled target of that kind.
                let target = match s("targetId") {
                    Some(id) => self.get::<Target>("targets", &id).ok_or("no such target")?,
                    None => {
                        let kind = s("channel").ok_or("targetId or channel required")?;
                        self.targets().into_iter().find(|t| t.kind == kind).ok_or_else(|| format!("no {kind} target yet (Settings › Post to)"))?
                    }
                };
                let channel = target.kind.clone();
                let fmt = if target.format.is_empty() { default_format(&channel).to_string() } else { target.format.clone() };
                let at = s("at").ok_or("at required")?;
                let cand = s("candidateId").and_then(|c| self.get::<Candidate>("candidates", &c));
                let render_id = s("renderId").or_else(|| cand.as_ref().and_then(|c| self.all::<Render>("renders").into_iter().find(|r| r.candidate_id == c.id && r.status == "done" && r.format == fmt).map(|r| r.id)));
                let src = cand.as_ref().and_then(|c| self.get::<Source>("sources", &c.source_id));
                let mut p = Post {
                    id: new_id("p"),
                    render_id,
                    candidate_id: cand.as_ref().map(|c| c.id.clone()),
                    poster_id: s("posterId"),
                    channel,
                    target_id: target.id.clone(),
                    scheduled_at: at,
                    status: if a["confirmed"].as_bool().unwrap_or(false) { "confirmed" } else { "planned" }.into(),
                    title: s("title").or_else(|| cand.as_ref().map(|c| c.title.clone())).unwrap_or_default(),
                    caption: s("caption").or_else(|| cand.as_ref().map(|c| format!("{}\n{}", c.caption, c.hashtags.iter().map(|h| format!("#{h}")).collect::<Vec<_>>().join(" ")))).unwrap_or_default(),
                    created_at: now(),
                    ..Default::default()
                };
                if let Some(src) = src {
                    p.caption = p.caption.replace("{source_url}", src.url.as_deref().unwrap_or(""));
                }
                self.save_post(&mut p);
                if p.status == "confirmed" {
                    self.enqueue("publish", &p.id, &p.title, json!({}));
                }
                json!(p)
            }
            "update_post" => {
                let id = id()?;
                let mut p: Post = self.get("posts", &id).ok_or("no such post")?;
                let mut v = serde_json::to_value(&p).unwrap();
                merge(&mut v, &a["patch"]);
                p = serde_json::from_value(v).map_err(|e| e.to_string())?;
                self.save_post(&mut p);
                json!(p)
            }
            "confirm" | "publish_now" => {
                let id = id()?;
                let mut p: Post = self.get("posts", &id).ok_or("no such post")?;
                if action == "publish_now" {
                    p.scheduled_at = now();
                }
                p.status = "confirmed".into();
                p.error = None;
                self.save_post(&mut p);
                json!(self.enqueue("publish", &p.id, &p.title, json!({})))
            }
            "set_thumbnail" => {
                // Send the reel's cover as the thumbnail of a posted YouTube video. id = post id.
                let id = id()?;
                let p: Post = self.get("posts", &id).ok_or("no such post")?;
                let video = p.provider_id.clone().ok_or("not posted yet")?;
                let target = self.target_for(&p).ok_or("no target")?;
                let cover = p.render_id.as_ref().and_then(|r| self.get::<Render>("renders", r)).and_then(|r| r.cover_path).ok_or("no cover for this post")?;
                youtube::set_thumbnail(&self.youtube_auth(&target), &video, Path::new(&cover))?;
                json!({ "video": video, "cover": cover })
            }
            "set_privacy" => {
                // Change privacy on a posted YouTube video: id = post id, privacy = public | unlisted | private.
                let id = id()?;
                let p: Post = self.get("posts", &id).ok_or("no such post")?;
                let video = p.provider_id.clone().ok_or("not posted yet")?;
                let target = self.target_for(&p).ok_or("no target")?;
                if target.kind != "youtube" {
                    return Err("only YouTube posts".into());
                }
                let privacy = s("privacy").unwrap_or_else(|| "public".into());
                youtube::set_privacy(&self.youtube_auth(&target), &video, &privacy)?;
                json!({ "video": video, "privacy": privacy })
            }
            "unschedule" => {
                let id = id()?;
                let _ = self.db.lock().delete("posts", &id);
                self.changed();
                json!(true)
            }
            "autofill" => json!(pipeline::autofill(self, a["days"].as_i64().unwrap_or(7))),
            "create_poster" => {
                let mut p = Poster { id: new_id("po"), template: s("template").unwrap_or_else(|| "weekly-tafsir".into()), title: s("title").unwrap_or_else(|| "Poster".into()), fields: a["fields"].clone(), status: "draft".into(), created_at: now(), ..Default::default() };
                if !p.fields.is_object() {
                    p.fields = json!({});
                }
                p.folder = self.out_dir().join("posters").join(format!("{}-{}", slug(&format!("{} {}", p.title, p.fields["date"].as_str().unwrap_or(""))), &p.id[3..9])).display().to_string();
                std::fs::create_dir_all(&p.folder).map_err(|e| e.to_string())?;
                self.save_poster(&mut p);
                json!(p)
            }
            "update_poster" => {
                let id = id()?;
                let mut p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                let mut v = serde_json::to_value(&p).unwrap();
                merge(&mut v, &a["patch"]);
                p = serde_json::from_value(v).map_err(|e| e.to_string())?;
                self.save_poster(&mut p);
                json!(p)
            }
            "remove_poster" => {
                let _ = self.db.lock().delete("posters", &id()?);
                self.changed();
                json!(true)
            }
            "generate_poster" => {
                let id = id()?;
                let p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                json!(self.enqueue("poster", &id, &p.title, json!({})))
            }
            "add_variant" => {
                let id = id()?;
                let mut p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                let path = s("path").ok_or("path required")?;
                if !Path::new(&path).exists() {
                    return Err("no such file".into());
                }
                let qr = self.settings().whatsapp_link;
                let link = p.fields["qrLink"].as_str().map(String::from).unwrap_or(qr);
                let final_path = if link.is_empty() {
                    path.clone()
                } else {
                    match posters::add_qr(&self.data_dir, Path::new(&path), &link) {
                        Ok(q) => {
                            p.qr_ok = true;
                            q.display().to_string()
                        }
                        Err(e) => {
                            log::warn!("QR paste failed on {path}: {e}");
                            path
                        }
                    }
                };
                p.variants.push(final_path);
                self.save_poster(&mut p);
                json!(p)
            }
            "choose_variant" => {
                let id = id()?;
                let mut p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                p.chosen = s("path");
                p.status = "chosen".into();
                self.save_poster(&mut p);
                json!(p)
            }
            "export_poster" => {
                let id = id()?;
                let mut p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                let outputs = posters::export(&p, &self.settings())?;
                if !p.outputs.is_object() {
                    p.outputs = json!({});
                }
                merge(&mut p.outputs, &outputs);
                p.status = "exported".into();
                self.save_poster(&mut p);
                if let Some(pal) = s("palette") {
                    let mut st = self.settings();
                    st.poster_history.push(pal);
                    self.save_settings(&st);
                }
                json!(p)
            }
            "end_cards" => {
                let id = id()?;
                let p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                json!(self.enqueue("endcards", &id, &format!("{} · end cards", p.title), json!({})))
            }
            "use_end_cards" => {
                // From a poster's outputs (or explicit paths) into settings, and turn the card on.
                let mut st = self.settings();
                let paths = if let Some(id) = s("id") {
                    let p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                    st.end_card.poster_id = Some(id);
                    json!({ "9x16": p.outputs["endcard-9x16"], "4x5": p.outputs["endcard-4x5"], "16x9": p.outputs["endcard-16x9"] })
                } else {
                    a["paths"].clone()
                };
                st.end_card.paths = paths;
                st.end_card.enabled = a["enabled"].as_bool().unwrap_or(true);
                if let Some(sec) = a["seconds"].as_f64() {
                    st.end_card.seconds = sec;
                }
                self.save_settings(&st);
                json!(st.end_card)
            }
            "waiting_video" => {
                let id = id()?;
                let p: Poster = self.get("posters", &id).ok_or("no such poster")?;
                json!(self.enqueue("waiting_video", &id, &format!("{} · waiting video", p.title), a.clone()))
            }
            "settings" => {
                let mut st = self.settings();
                if a["patch"].is_object() {
                    let mut v = serde_json::to_value(&st).unwrap();
                    merge(&mut v, &a["patch"]);
                    st = serde_json::from_value(v).map_err(|e| e.to_string())?;
                    if st.caption_templates.is_empty() {
                        st.caption_templates = model::caption_templates();
                    }
                    self.save_settings(&st);
                    let _ = std::fs::create_dir_all(&st.out_dir);
                }
                json!(st)
            }
            "tools" => json!(tools::detect(&self.settings().claude_bin)),
            "install_whisper" => json!(tools::install_whisper()?),
            "add_target" => {
                let kind = s("kind").unwrap_or_else(|| "youtube".into());
                let mut t = Target { id: new_id("t"), kind: kind.clone(), name: s("name").unwrap_or_else(|| match kind.as_str() { "youtube" => "YouTube".into(), "instagram" => "Instagram".into(), "tiktok" => "TikTok".into(), "facebook" => "Facebook".into(), _ => "Folder".into() }), path: s("path").unwrap_or_default(), hours: a["hours"].as_array().map(|h| h.iter().filter_map(|v| v.as_i64()).collect()).unwrap_or_default(), ..Default::default() };
                if kind != "youtube" && kind != "folder" {
                    t.title_suffix = String::new();
                }
                self.save_target(&mut t);
                json!(t)
            }
            "update_target" => {
                let id = id()?;
                let mut t: Target = self.get("targets", &id).ok_or("no such target")?;
                let mut v = serde_json::to_value(&t).unwrap();
                merge(&mut v, &a["patch"]);
                t = serde_json::from_value(v).map_err(|e| e.to_string())?;
                self.save_target(&mut t);
                json!(t)
            }
            "remove_target" => {
                let id = id()?;
                let _ = self.db.lock().delete("targets", &id);
                self.changed();
                json!(true)
            }
            "target_connect" => {
                // YouTube: the browser opens, the login lands on this target.
                let id = id()?;
                let mut t: Target = self.get("targets", &id).ok_or("no such target")?;
                let auth = youtube::connect(&self.youtube_auth(&t))?;
                t.refresh_token = auth.refresh_token;
                t.channel_title = auth.channel_title.clone();
                if t.name.is_empty() || t.name == "YouTube" {
                    t.name = auth.channel_title;
                }
                self.save_target(&mut t);
                json!(t)
            }
            "target_disconnect" => {
                let id = id()?;
                let mut t: Target = self.get("targets", &id).ok_or("no such target")?;
                t.refresh_token.clear();
                t.channel_title.clear();
                self.save_target(&mut t);
                json!(t)
            }
            "youtube_connect" => {
                // Kept for the CLI: connects the first YouTube target, creating one if needed.
                let mut t = self.all::<Target>("targets").into_iter().find(|t| t.kind == "youtube").unwrap_or_else(|| Target { id: new_id("t"), kind: "youtube".into(), name: "YouTube".into(), ..Default::default() });
                let auth = youtube::connect(&self.youtube_auth(&t))?;
                t.refresh_token = auth.refresh_token;
                t.channel_title = auth.channel_title.clone();
                if t.name == "YouTube" {
                    t.name = auth.channel_title;
                }
                self.save_target(&mut t);
                json!(t)
            }
            "youtube_disconnect" => {
                for mut t in self.all::<Target>("targets").into_iter().filter(|t| t.kind == "youtube") {
                    t.refresh_token.clear();
                    t.channel_title.clear();
                    self.save_target(&mut t);
                }
                json!(true)
            }
            "shutdown" => {
                // Drain: claim nothing new, let the active jobs finish, then exit. Queued jobs resume at the next start.
                self.draining.store(true, std::sync::atomic::Ordering::SeqCst);
                let active = self.active.lock().len();
                log::info!("shutdown requested: draining {active} active jobs");
                let lib_active = std::sync::Arc::new(Mutex::new(()));
                let _ = lib_active;
                let wait = a["waitSeconds"].as_u64().unwrap_or(1800);
                let me: &'static Library = unsafe { &*(self as *const Library) }; // the process exits from this thread; the Library outlives it
                std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
                    while !me.active.lock().is_empty() && std::time::Instant::now() < deadline {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    log::info!("shutdown: exiting");
                    std::process::exit(0);
                });
                json!({ "draining": true, "active": active })
            }
            "mcp" => json!(self.mcp.get().map(|m| json!({ "url": m.url(), "token": m.token }))),
            "paths" => json!({ "dataDir": self.data_dir, "outDir": self.out_dir(), "log": self.data_dir.join("cuttar.log") }),
            "open" => {
                let p = s("path").ok_or("path required")?;
                let mut cmd = std::process::Command::new("open");
                if a["reveal"].as_bool().unwrap_or(false) {
                    cmd.arg("-R");
                }
                let _ = cmd.arg(&p).spawn();
                json!(true)
            }
            "search" => {
                let q = s("query").unwrap_or_default();
                let ids = self.db.lock().fts_search(&q).unwrap_or_default();
                json!(ids)
            }
            _ => return Err(format!("unknown action {action}")),
        };
        Ok(out)
    }
}

/// The render format a channel takes by default.
pub fn default_format(channel: &str) -> &'static str {
    match channel {
        "tiktok" => "tiktok",
        "instagram" | "facebook" => "reels",
        _ => "shorts",
    }
}

/// Shallow merge of `patch` into `base` (objects only, top level).
pub fn merge(base: &mut Value, patch: &Value) {
    if let (Some(b), Some(p)) = (base.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            if v.is_object() && b.get(k).map(|x| x.is_object()).unwrap_or(false) {
                merge(b.get_mut(k).unwrap(), v);
            } else {
                b.insert(k.clone(), v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_source_and_candidates_flow() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let lib = Library::open(dir.path(), None).unwrap();
        lib.save_settings(&Settings { out_dir: out.display().to_string(), parallel_jobs: 1, ..Default::default() });
        let src = lib.dispatch("add_source", json!({ "url": "https://www.youtube.com/watch?v=abc", "language": "nl" })).unwrap();
        let id = src["id"].as_str().unwrap().to_string();
        assert!(lib.dispatch("add_source", json!({ "url": "https://www.youtube.com/watch?v=abc" })).is_err());
        let jobs = lib.all::<Job>("jobs");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, "download");
        let c = lib.dispatch("add_candidate", json!({ "sourceId": id, "start": 10.0, "end": 100.0, "title": "x" })).unwrap();
        assert_eq!(c["end"].as_f64().unwrap(), 50.0);
        let cid = c["id"].as_str().unwrap();
        assert!(lib.dispatch("update_candidate", json!({ "id": cid, "patch": { "end": 80.0 } })).is_err());
        lib.dispatch("approve", json!({ "ids": [cid] })).unwrap();
        let jobs = lib.dispatch("render", json!({ "sourceId": id })).unwrap();
        assert_eq!(jobs.as_array().unwrap().len(), 3);
        let snap = lib.snapshot();
        assert_eq!(snap["candidates"].as_array().unwrap().len(), 1);
        assert_eq!(snap["sources"][0]["langOverride"], "nl");
        let mut v = json!({ "a": 1, "o": { "x": 1, "y": 2 } });
        merge(&mut v, &json!({ "o": { "y": 3 }, "b": 2 }));
        assert_eq!(v, json!({ "a": 1, "b": 2, "o": { "x": 1, "y": 3 } }));
    }
}
