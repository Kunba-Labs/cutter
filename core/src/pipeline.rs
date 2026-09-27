//! Jobs: the workers that run them, what each kind does, and the scheduler
//! that checks channels and sends posts on time.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::model::*;
use crate::{brain, captions, ffmpeg, posters, whisper, youtube, ytdlp, Library};

pub fn worker(lib: Arc<Library>) {
    loop {
        match lib.claim_job() {
            Some(job) => {
                let id = job.id.clone();
                log::info!("job {} {} {} start", id, job.kind, job.ref_id);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&lib, &job))).unwrap_or_else(|_| Err("job panicked".into()));
                if let Err(e) = &result {
                    log::warn!("job {id} failed: {e}");
                }
                lib.finish_job(&id, result);
            }
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// Every 30 s: channels past their interval, and confirmed posts whose time came.
pub fn scheduler(lib: Arc<Library>) {
    std::thread::sleep(Duration::from_secs(5));
    loop {
        let now = chrono::Utc::now();
        for c in lib.all::<Channel>("channels").into_iter().filter(|c| c.enabled) {
            let due = c.last_check.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| now.signed_duration_since(t).num_minutes() >= c.interval_h.max(1) * 60).unwrap_or(true);
            if due {
                lib.enqueue("check_channel", &c.id, &c.name, json!({}));
            }
        }
        for p in lib.all::<Post>("posts") {
            // YouTube takes publishAt, so a confirmed YouTube post uploads right away; others wait for their time.
            let due = p.channel == "youtube" || chrono::DateTime::parse_from_rfc3339(&p.scheduled_at).map(|t| t <= now).unwrap_or(false);
            if p.status == "confirmed" && due {
                lib.enqueue("publish", &p.id, &p.title, json!({}));
            }
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

fn run(lib: &Library, job: &Job) -> Result<Value, String> {
    match job.kind.as_str() {
        "download" => download(lib, job),
        "transcribe" => transcribe(lib, job),
        "detect" => detect(lib, job),
        "polish" => polish(lib, job),
        "render" => render(lib, job),
        "publish" => publish(lib, job),
        "check_channel" => check_channel(lib, job),
        "poster" => poster(lib, job),
        "endcards" => endcards(lib, job),
        "waiting_video" => waiting_video(lib, job),
        k => Err(format!("unknown job kind {k}")),
    }
}

fn source(lib: &Library, id: &str) -> Result<Source, String> {
    lib.get::<Source>("sources", id).ok_or_else(|| "source is gone".to_string())
}

/// The brain picks the cover from nine numbered frames of the cut; the choice is kept on the
/// candidate so the other formats reuse it. Falls back to one second in.
fn pick_cover(lib: &Library, job: &Job, s: &Settings, c: &Candidate, video: &Path, tmp: &Path, g: (i64, i64, i64, i64, f64, f64, f64)) -> f64 {
    let fallback = c.start + 1.0;
    lib.job_progress(&job.id, 0.97, "choosing the cover");
    // An older render job of the same reel may be asking already: wait for its answer instead of asking twice.
    let older = lib.all::<Job>("jobs").into_iter().any(|j| j.kind == "render" && j.ref_id == c.id && j.status == "running" && j.created_at < job.created_at);
    if older {
        for _ in 0..90 {
            std::thread::sleep(Duration::from_secs(2));
            if let Some(t) = lib.get::<Candidate>("candidates", &c.id).and_then(|cc| cc.cover_t) {
                return t;
            }
            if !lib.all::<Job>("jobs").into_iter().any(|j| j.kind == "render" && j.ref_id == c.id && j.status == "running" && j.created_at < job.created_at) {
                break;
            }
        }
    }
    let (sw, sh, w, h, cx, cy, cz) = g;
    let picked = (|| -> Result<f64, String> {
        let (sheet, times) = ffmpeg::cover_sheet(video, c.start, c.end, tmp, sw, sh, w, h, cx, cy, cz)?;
        let text = brain::ask(s, &brain::cover_prompt(&sheet, &c.title), Duration::from_secs(180), |pid| lib.register_child(&job.id, pid))?;
        // The sheet stays beside the reel so the choice can be checked.
        if let Some(parent) = tmp.parent() {
            let _ = std::fs::rename(&sheet, parent.join("cover-sheet.jpg"));
        }
        let n = brain::parse_frame(&text).ok_or_else(|| format!("no frame in: {}", text.chars().take(120).collect::<String>()))?;
        Ok(times[n - 1])
    })();
    let _ = std::fs::remove_dir_all(tmp);
    match picked {
        Ok(t) => {
            if let Some(mut cc) = lib.get::<Candidate>("candidates", &c.id) {
                cc.cover_t = Some(t);
                lib.save_candidate(&mut cc);
            }
            t
        }
        Err(e) => {
            log::warn!("cover pick for {}: {e}", c.id);
            fallback
        }
    }
}

/// The reel's caption template by name (else the library default), its caption position, and
/// the title look borrowed from the title template.
pub fn reel_style(s: &Settings, c: &Candidate) -> CaptionStyle {
    let mut style = c.style.as_deref().and_then(|name| s.caption_templates.iter().find(|t| t.name == name).cloned()).unwrap_or(s.caption_style.clone());
    if let Some(p) = c.caption_pct {
        style.position_pct = p.clamp(4, 70);
    }
    if let Some(h) = c.hook_style.as_deref().and_then(|name| s.caption_templates.iter().find(|t| t.name == name)) {
        style.hook_font = h.hook_font.clone();
        style.hook_size = h.hook_size;
        style.hook_color = h.hook_color.clone();
        style.hook_boxed = h.hook_boxed;
        style.hook_box_color = h.hook_box_color.clone();
        style.hook_uppercase = h.hook_uppercase;
        style.hook_bold = h.hook_bold;
        style.hook_italic = h.hook_italic;
        style.hook_pct = h.hook_pct;
        style.hook_seconds = h.hook_seconds;
        style.hook = h.hook;
    }
    style
}

/// `{format}-cover.jpg` for a reel: the chosen frame (else one second in), the reel's crop,
/// and the styled title burned in. Cheap, so it can be redone when a title changes.
pub fn make_cover(lib: &Library, c: &Candidate, format: &str) -> Result<PathBuf, String> {
    let s = lib.settings();
    let src = source(lib, &c.source_id)?;
    let video = PathBuf::from(src.video_path.clone().ok_or("no video")?);
    let (w, h, extra) = ffmpeg::format_spec(format);
    let (sw, sh) = (src.meta["width"].as_i64().unwrap_or(1920), src.meta["height"].as_i64().unwrap_or(1080));
    let (cx, cy, cz) = (c.crop["x"].as_f64().unwrap_or(0.5), c.crop["y"].as_f64().unwrap_or(0.5), c.crop["z"].as_f64().unwrap_or(1.0));
    let dir = PathBuf::from(&src.folder).join(slug(&c.title));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let language = lib.get::<Transcript>("transcripts", &src.id).map(|t| t.language).unwrap_or_default();
    let title = if c.hook.trim().is_empty() { c.title.as_str() } else { c.hook.as_str() };
    let ass = dir.join(format!("{format}-cover.ass"));
    std::fs::write(&ass, captions::cover(&reel_style(&s, c), w, h, extra, &language, title, &s.channel_name)).map_err(|e| e.to_string())?;
    let out = dir.join(format!("{format}-cover.jpg"));
    ffmpeg::cover(&video, c.cover_t.unwrap_or(c.start + 1.0), &out, sw, sh, w, h, cx, cy, cz, Some(&ass))?;
    Ok(out)
}

fn set_stage(lib: &Library, src: &mut Source, stage: &str, error: Option<String>) {
    src.stage = stage.into();
    src.error = error;
    lib.save_source(src);
}

fn fail(lib: &Library, id: &str, e: String) -> Result<Value, String> {
    if let Ok(mut src) = source(lib, id) {
        set_stage(lib, &mut src, "failed", Some(e.clone()));
    }
    Err(e)
}

fn download(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut src = source(lib, &job.ref_id)?;
    std::fs::create_dir_all(&src.folder).map_err(|e| e.to_string())?;
    let dir = PathBuf::from(&src.folder);
    // `keep`: fetch the stream again but leave transcript, reels and edits alone.
    let keep = job.args["keep"].as_bool().unwrap_or(false);
    let stage_before = src.stage.clone();
    let prev = dir.join("source.prev.mp4");
    if keep {
        let _ = std::fs::rename(dir.join("source.mp4"), &prev);
    } else {
        set_stage(lib, &mut src, "downloading", None);
    }
    let res: Result<(), String> = (|| {
        if let Some(p) = src.path.clone() {
            // A local file: link it into the folder so every source looks the same.
            let video = dir.join("source.mp4");
            if !video.exists() {
                std::fs::hard_link(&p, &video).or_else(|_| std::fs::copy(&p, &video).map(|_| ())).map_err(|e| format!("copy: {e}"))?;
            }
            let thumb = dir.join("thumb.jpg");
            let _ = ffmpeg::thumbnail(&video, &thumb);
            src.video_path = Some(video.display().to_string());
            src.thumb_path = Some(thumb.display().to_string()).filter(|_| thumb.exists());
        } else {
            let url = src.url.clone().unwrap();
            lib.job_progress(&job.id, 0.0, "reading metadata");
            let info = ytdlp::info(&url)?;
            src.title = info["title"].as_str().unwrap_or(&src.title).to_string();
            src.channel = info["channel"].as_str().or(info["uploader"].as_str()).map(String::from);
            src.duration = info["duration"].as_f64();
            if src.lang_override.is_none() {
                src.language = info["language"].as_str().map(|l| l[..2.min(l.len())].to_string());
            }
            src.meta = json!({ "uploadDate": info["upload_date"], "description": info["description"].as_str().map(|d| d.chars().take(2000).collect::<String>()), "chapters": info["chapters"], "viewCount": info["view_count"], "id": info["id"] });
            lib.save_source(&mut src);
            let lang = src.lang_override.clone().or(src.language.clone());
            let jid = job.id.clone();
            let d = ytdlp::download(&url, &dir, lang.as_deref(), |p, msg| lib.job_progress(&jid, p, msg), |pid| lib.register_child(&job.id, pid))?;
            src.video_path = Some(d.video.display().to_string());
            src.thumb_path = d.thumb.map(|p| p.display().to_string());
            src.captions_path = d.captions.map(|p| p.display().to_string());
        }
        let video = PathBuf::from(src.video_path.as_ref().unwrap());
        let probe = ffmpeg::probe(&video)?;
        src.duration = Some(probe.duration);
        src.meta["width"] = json!(probe.width);
        src.meta["height"] = json!(probe.height);
        src.meta["hasAudio"] = json!(probe.has_audio);
        src.meta["fps"] = json!(probe.fps);
        src.meta["vcodec"] = json!(probe.vcodec);
        // The in-app player wants H.264/AAC at 1080p or less; anything else gets a preview copy.
        let preview = dir.join("preview.mp4");
        let _ = std::fs::remove_file(&preview);
        src.preview_path = None;
        if probe.vcodec != "h264" || probe.width.max(probe.height) > 1920 || (probe.has_audio && probe.acodec != "aac") {
            let jid = job.id.clone();
            ffmpeg::proxy(&video, &preview, probe.duration, |p, m| lib.job_progress(&jid, p, &format!("preview copy {m}")))?;
            src.preview_path = Some(preview.display().to_string());
        }
        Ok(())
    })();
    if keep {
        match &res {
            Ok(()) => {
                let _ = std::fs::remove_file(&prev);
                src.stage = stage_before;
                lib.save_source(&mut src);
                // Every ticked reel is rendered from the new file, files from the old one included.
                let n = lib.dispatch("render", json!({ "sourceId": src.id })).ok().and_then(|v| v.as_array().map(|a| a.len())).unwrap_or(0);
                return Ok(json!({ "message": format!("{}p {} · edits kept · {n} reels rendering again", src.meta["height"], src.meta["vcodec"].as_str().unwrap_or("")) }));
            }
            Err(e) => {
                let _ = std::fs::rename(&prev, dir.join("source.mp4"));
                return Err(e.clone());
            }
        }
    }
    if let Err(mut e) = res {
        // A 403 on a video that lists fine is YouTube moving on from an old yt-dlp.
        if e.contains("403") {
            e.push_str(" — YouTube changed something; update yt-dlp (`brew upgrade yt-dlp`) and run again");
        }
        return fail(lib, &job.ref_id, e);
    }
    set_stage(lib, &mut src, "downloaded", None);
    lib.enqueue("transcribe", &src.id, &src.title, json!({}));
    Ok(json!({ "message": "downloaded" }))
}

fn transcribe(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut src = source(lib, &job.ref_id)?;
    let video = PathBuf::from(src.video_path.clone().ok_or("no video yet")?);
    let s = lib.settings();
    set_stage(lib, &mut src, "transcribing", None);
    // The override wins; otherwise YouTube's declared language is a better prior
    // than whisper's 30-second guess (a Dutch lecture opening with Qur'an comes out "ar").
    let lang = src.lang_override.clone().or_else(|| src.language.clone().filter(|l| l.len() == 2));
    let res: Result<Transcript, String> = (|| {
        let use_captions = matches!(src.transcript_source.as_str(), "captions" | "both");
        let captions = if use_captions {
            src.captions_path.clone().or_else(|| src.url.as_ref().and_then(|u| ytdlp::fetch_auto_captions(u, video.parent().unwrap(), lang.as_deref().or(src.language.as_deref()).unwrap_or("nl")).map(|p| p.display().to_string())))
        } else {
            None
        };
        if src.transcript_source == "captions" {
            let path = captions.ok_or("YouTube has no captions for this video; switch to whisper")?;
            let vtt = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let segments = ytdlp::vtt_to_segments(&vtt);
            if segments.is_empty() {
                return Err("the caption file is empty".into());
            }
            let language = lang.clone().or(src.language.clone()).unwrap_or_else(|| Path::new(&path).file_name().unwrap().to_string_lossy().split('.').nth(1).unwrap_or("").chars().take(2).collect());
            return Ok(Transcript { source_id: src.id.clone(), engine: "captions".into(), language, segments, created_at: now() });
        }
        let jid = job.id.clone();
        let attempt = |model: &str| whisper::transcribe(&video, model, lang.as_deref(), &s.glossary, src.duration.unwrap_or(0.0), |p, m| lib.job_progress(&jid, p, &format!("{model} · {m}")), |pid| lib.register_child(&jid, pid));
        let t = match attempt(&s.whisper_model) {
            Ok(t) => t,
            Err(e) if whisper::is_oom(&e) && !s.whisper_fallback.is_empty() => {
                lib.job_log(&job.id, &format!("{e} — retrying with {}", s.whisper_fallback));
                attempt(&s.whisper_fallback)?
            }
            Err(e) => return Err(e),
        };
        if t.segments.is_empty() {
            return Err("whisper heard nothing".into());
        }
        Ok(Transcript { source_id: src.id.clone(), engine: format!("whisper {}", s.whisper_model.rsplit('/').next().unwrap_or("")), language: lang.clone().unwrap_or(t.language), segments: t.segments, created_at: now() })
    })();
    let t = match res {
        Ok(t) => t,
        Err(e) => return fail(lib, &job.ref_id, e),
    };
    let text: String = t.segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    lib.put("transcripts", &src.id, &now(), &t);
    let _ = lib.db.lock().fts_put(&src.id, &text);
    let _ = std::fs::write(Path::new(&src.folder).join("transcript.srt"), captions::srt(&t.segments, 0.0, f64::MAX));
    src.language = Some(t.language.clone());
    set_stage(lib, &mut src, "transcribed", None);
    if s.polish_captions {
        lib.enqueue("polish", &src.id, &src.title, json!({ "thenDetect": src.auto_detect }));
    } else if src.auto_detect {
        lib.enqueue("detect", &src.id, &src.title, json!({}));
    }
    Ok(json!({ "message": format!("{} segments · {}", t.segments.len(), t.language) }))
}

fn detect(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut src = source(lib, &job.ref_id)?;
    let t: Transcript = lib.get("transcripts", &src.id).ok_or("transcribe first")?;
    let s = lib.settings();
    set_stage(lib, &mut src, "detecting", None);
    let mut drafts = Vec::new();
    let chunks = brain::chunks(&t.segments);
    for (n, (a, b)) in chunks.iter().enumerate() {
        lib.job_progress(&job.id, n as f64 / chunks.len() as f64, &format!("{} reading part {} of {}", s.brain, n + 1, chunks.len()));
        let prompt = brain::prompt(&t.segments[*a..*b], *a, &src.title, &t.language, &s);
        let text = match brain::ask(&s, &prompt, Duration::from_secs(15 * 60), |pid| lib.register_child(&job.id, pid)) {
            Ok(t) => t,
            Err(e) => return fail(lib, &src.id, e),
        };
        match brain::parse_drafts(&text) {
            Ok(d) => drafts.extend(d),
            Err(e) => {
                lib.job_log(&job.id, &format!("{e}: {}", text.chars().take(400).collect::<String>()));
                return fail(lib, &src.id, e);
            }
        }
    }
    let cands = brain::to_candidates(drafts, &t.segments, &src.id, s.max_reel_s as f64);
    // Replace the previous run's untouched candidates; keep approved and edited ones.
    let _ = lib.db.lock().delete_where::<Candidate>("candidates", |c| c.source_id == src.id && !c.approved && !c.discarded && c.created_at == c.updated_at);
    let mut n = 0;
    for mut c in cands {
        if c.score < s.min_score {
            continue;
        }
        if src.auto_approve_score > 0 && c.score >= src.auto_approve_score {
            c.approved = true;
        }
        lib.put("candidates", &c.id, &format!("{}-{:08.2}", c.source_id, c.start), &c);
        n += 1;
        if c.approved {
            for f in &s.formats {
                lib.enqueue("render", &c.id, &format!("{} · {f}", c.title), json!({ "format": f }));
            }
        }
    }
    set_stage(lib, &mut src, "review", None);
    Ok(json!({ "message": format!("{n} candidates") }))
}

fn render(lib: &Library, job: &Job) -> Result<Value, String> {
    let c: Candidate = lib.get("candidates", &job.ref_id).ok_or("candidate is gone")?;
    let src = source(lib, &c.source_id)?;
    let t: Transcript = lib.get("transcripts", &src.id).ok_or("no transcript")?;
    let s = lib.settings();
    let format = job.args["format"].as_str().unwrap_or("shorts").to_string();
    let video = PathBuf::from(src.video_path.clone().ok_or("no video")?);
    let (w, h, extra) = ffmpeg::format_spec(&format);
    let (sw, sh) = (src.meta["width"].as_i64().unwrap_or(1920), src.meta["height"].as_i64().unwrap_or(1080));
    let dir = PathBuf::from(&src.folder).join(slug(&c.title));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join(format!("{format}.mp4"));
    let lead = 0.25;
    let (start, end) = ((c.start - lead).max(0.0), (c.end + 0.35).min(src.duration.unwrap_or(f64::MAX)));
    let style = reel_style(&s, &c);
    let ass = dir.join(format!("{format}.ass"));
    // One caption language: the translation when one is chosen and this reel has it, else the spoken words.
    // The language decides font and size (RTL scripts), so it must match what is actually drawn.
    let use_translation = !s.translate_to.is_empty() && s.translate_to != t.language && c.translation.iter().any(|l| !l.text.trim().is_empty());
    let caption_language = if use_translation { s.translate_to.as_str() } else { t.language.as_str() };
    std::fs::write(&ass, captions::build(&captions::CaptionSpec { segments: &t.segments, translation: &c.translation, clip_start: start, clip_end: end, width: w, height: h, extra_bottom: extra, style: &style, language: caption_language, hook: &c.hook, watermark: &s.channel_name, use_translation })).map_err(|e| e.to_string())?;
    let mut r = Render { id: new_id("r"), candidate_id: c.id.clone(), source_id: src.id.clone(), format: format.clone(), path: out.display().to_string(), status: "rendering".into(), created_at: now(), ..Default::default() };
    lib.put("renders", &r.id, &r.created_at, &r);
    let jid = job.id.clone();
    let card_path = s.end_card.enabled.then(|| s.end_card.paths[ffmpeg::aspect_key(w, h)].as_str().map(PathBuf::from)).flatten().filter(|p| p.exists());
    let end_card = card_path.as_deref().map(|p| (p, s.end_card.seconds.clamp(0.5, 8.0)));
    let has_audio = src.meta["hasAudio"].as_bool().unwrap_or_else(|| ffmpeg::probe(&video).map(|p| p.has_audio).unwrap_or(true));
    let res = ffmpeg::render(
        &ffmpeg::RenderSpec { src: &video, start, end, out: &out, width: w, height: h, src_w: sw, src_h: sh, crop_x: c.crop["x"].as_f64().unwrap_or(0.5), crop_y: c.crop["y"].as_f64().unwrap_or(0.5), crop_z: c.crop["z"].as_f64().unwrap_or(1.0), ass: Some(&ass), end_card, has_audio, fps: src.meta["fps"].as_f64().unwrap_or(30.0), quality: &s.render_quality },
        |p, m| lib.job_progress(&jid, p, m),
        |pid| lib.register_child(&jid, pid),
    );
    match res {
        Ok(()) => {
            let (cx, cy, cz) = (c.crop["x"].as_f64().unwrap_or(0.5), c.crop["y"].as_f64().unwrap_or(0.5), c.crop["z"].as_f64().unwrap_or(1.0));
            let mut c = c.clone();
            if c.cover_t.is_none() {
                c.cover_t = Some(pick_cover(lib, job, &s, &c, &video, &dir.join(format!(".cover-{format}")), (sw, sh, w, h, cx, cy, cz)));
            }
            let cover = match make_cover(lib, &c, &format) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("cover for {}: {e}", c.id);
                    dir.join(format!("{format}-cover.jpg"))
                }
            };
            let srt = dir.join("captions.srt");
            let _ = std::fs::write(&srt, captions::srt(&t.segments, start, end));
            let _ = std::fs::write(dir.join("caption.txt"), format!("{}\n\n{}\n{}", c.title, c.caption, c.hashtags.iter().map(|h| format!("#{h}")).collect::<Vec<_>>().join(" ")));
            r.status = "done".into();
            r.cover_path = Some(cover.display().to_string()).filter(|_| cover.exists());
            r.srt_path = Some(srt.display().to_string());
            // Older renders of the same reel+format are replaced.
            let _ = lib.db.lock().delete_where::<Render>("renders", |o| o.candidate_id == c.id && o.format == format && o.id != r.id);
            lib.put("renders", &r.id, &r.created_at, &r);
            if src.auto_schedule {
                autofill(lib, 14);
            }
            lib.changed();
            Ok(json!({ "message": out.display().to_string() }))
        }
        Err(e) => {
            r.status = "failed".into();
            r.error = Some(e.clone());
            lib.put("renders", &r.id, &r.created_at, &r);
            lib.changed();
            Err(e)
        }
    }
}

fn publish(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut p: Post = lib.get("posts", &job.ref_id).ok_or("post is gone")?;
    if p.status == "posted" {
        return Ok(json!({ "message": "already posted" }));
    }
    p.status = "posting".into();
    lib.save_post(&mut p);
    let s = lib.settings();
    let res: Result<String, String> = (|| {
        let path = match (&p.render_id, &p.poster_id) {
            (Some(r), _) => lib.get::<Render>("renders", r).map(|r| r.path).ok_or("the render is gone")?,
            (None, Some(po)) => lib.get::<Poster>("posters", po).and_then(|p| p.outputs["story"].as_str().map(String::from)).ok_or("export the poster first")?,
            _ => return Err("nothing to post: render the reel first".into()),
        };
        match p.channel.as_str() {
            "youtube" => {
                let publish_at = chrono::DateTime::parse_from_rfc3339(&p.scheduled_at).ok().filter(|t| *t > chrono::Utc::now() + chrono::Duration::minutes(2)).map(|t| t.to_rfc3339());
                let jid = job.id.clone();
                let title = format!("{}{}", p.title.chars().take(95).collect::<String>(), s.youtube.title_suffix);
                let tags: Vec<String> = p.caption.split_whitespace().filter_map(|w| w.strip_prefix('#')).map(String::from).collect();
                youtube::upload(&s.youtube, &youtube::Upload { path: Path::new(&path), title: &title, description: &p.caption, tags: &tags, category_id: &s.youtube.category_id, privacy: &s.youtube.privacy, publish_at: publish_at.as_deref() }, |pr, m| lib.job_progress(&jid, pr, m))
            }
            // ponytail: TikTok and Meta need approved developer apps; until then the render sits in Finder with its caption.txt.
            other => Err(format!("{other} is not linked yet. The file is ready at {path} with caption.txt beside it.")),
        }
    })();
    match res {
        Ok(provider_id) => {
            p.status = "posted".into();
            p.provider_id = Some(provider_id.clone());
            p.error = None;
            lib.save_post(&mut p);
            Ok(json!({ "message": provider_id }))
        }
        Err(e) => {
            p.status = "failed".into();
            p.error = Some(e.clone());
            lib.save_post(&mut p);
            Err(e)
        }
    }
}

fn check_channel(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut c: Channel = lib.get("channels", &job.ref_id).ok_or("channel is gone")?;
    let videos = match ytdlp::channel_videos(&c.url, 30) {
        Ok(v) => v,
        Err(e) => {
            c.last_error = Some(e.clone());
            c.last_check = Some(now());
            lib.put("channels", &c.id, &now(), &c);
            lib.changed();
            return Err(e);
        }
    };
    let known: Vec<InboxItem> = lib.all::<InboxItem>("inbox").into_iter().filter(|i| i.channel_id == c.id).collect();
    let re = (!c.title_regex.trim().is_empty()).then(|| regex::RegexBuilder::new(&c.title_regex).case_insensitive(true).build().ok()).flatten();
    let mut new = 0;
    for v in videos {
        if known.iter().any(|k| k.video_id == v.id) {
            continue;
        }
        let below = v.duration.map(|d| d < c.min_len_s as f64).unwrap_or(false);
        let filtered = re.as_ref().map(|r| !r.is_match(&v.title)).unwrap_or(false);
        let too_old = !c.since.is_empty() && v.uploaded_at.as_deref().map(|u| u < c.since.as_str()).unwrap_or(false);
        let status = if below { "below_min" } else if filtered { "filtered" } else if too_old { "skipped" } else { "new" };
        let mut item = InboxItem { id: new_id("in"), channel_id: c.id.clone(), video_id: v.id, url: v.url, title: v.title, duration: v.duration, uploaded_at: v.uploaded_at, status: status.into(), source_id: None, found_at: now() };
        if status == "new" && c.auto_process {
            if let Ok(src) = lib.add_source(&json!({ "url": item.url, "title": item.title, "channelId": c.id, "autoApproveScore": c.auto_approve_score, "autoSchedule": c.auto_schedule })) {
                item.status = "processed".into();
                item.source_id = Some(src.id);
            }
        }
        if status == "new" {
            new += 1;
        }
        lib.put("inbox", &item.id, &item.found_at, &item);
    }
    c.last_check = Some(now());
    c.last_error = None;
    if c.name == c.url {
        if let Some(first) = known.first() {
            let _ = first;
        }
    }
    lib.put("channels", &c.id, &now(), &c);
    lib.changed();
    Ok(json!({ "message": format!("{new} new") }))
}

fn poster(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut p: Poster = lib.get("posters", &job.ref_id).ok_or("poster is gone")?;
    p.status = "generating".into();
    lib.save_poster(&mut p);
    lib.job_progress(&job.id, 0.05, "Claude + Higgsfield are drawing 3 variants");
    let res = posters::generate(lib, &p, |pid| lib.register_child(&job.id, pid));
    let (files, palettes) = match res {
        Ok(v) => v,
        Err(e) => {
            p.status = "failed".into();
            p.error = Some(e.clone());
            lib.save_poster(&mut p);
            return Err(e);
        }
    };
    let link = p.fields["qrLink"].as_str().map(String::from).unwrap_or_else(|| lib.settings().whatsapp_link);
    let mut variants = Vec::new();
    for f in files {
        lib.job_progress(&job.id, 0.8, "pasting the QR code");
        let path = if link.is_empty() {
            f.display().to_string()
        } else {
            match posters::add_qr(&lib.data_dir, &f, &link) {
                Ok(q) => {
                    p.qr_ok = true;
                    q.display().to_string()
                }
                Err(e) => {
                    lib.job_log(&job.id, &format!("QR failed on {}: {e}", f.display()));
                    f.display().to_string()
                }
            }
        };
        variants.push(path);
    }
    p.variants = variants;
    p.fields["palettes"] = json!(palettes);
    p.status = "variants".into();
    p.error = None;
    lib.save_poster(&mut p);
    let _ = std::process::Command::new("open").arg(&p.folder).spawn();
    Ok(json!({ "message": format!("{} variants", p.variants.len()) }))
}

fn waiting_video(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut p: Poster = lib.get("posters", &job.ref_id).ok_or("poster is gone")?;
    let a = &job.args;
    let start_at = a["startAt"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&chrono::Utc));
    let lines: Vec<String> = a["lines"].as_array().map(|l| l.iter().filter_map(|v| v.as_str().map(String::from)).collect()).filter(|l: &Vec<String>| !l.is_empty()).unwrap_or_else(|| {
        vec!["Begint zo".into(), p.title.clone(), p.fields["date"].as_str().unwrap_or("").to_string(), p.fields["programme"].as_str().unwrap_or("").to_string()].into_iter().filter(|l| !l.is_empty()).collect()
    });
    let args = posters::WaitingArgs { start_at, lines, loop_s: a["loopS"].as_i64().unwrap_or(600), countdown: a["countdown"].as_bool().unwrap_or(true) };
    let font = lib.settings().caption_style.font;
    let jid = job.id.clone();
    let outputs = posters::waiting_video(&p, &args, &font, |pr, m| lib.job_progress(&jid, pr, m))?;
    if !p.outputs.is_object() {
        p.outputs = json!({});
    }
    crate::merge(&mut p.outputs, &outputs);
    lib.save_poster(&mut p);
    Ok(json!({ "message": "waiting videos rendered" }))
}

/// Put approved, rendered, unscheduled reels on the calendar: one per channel
/// per day at the channel's cadence hours, starting tomorrow. Returns how many.
pub fn autofill(lib: &Library, days: i64) -> usize {
    let s = lib.settings();
    let posts: Vec<Post> = lib.all("posts");
    let renders: Vec<Render> = lib.all::<Render>("renders").into_iter().filter(|r| r.status == "done").collect();
    let mut n = 0;
    let cadence = s.cadence.as_object().cloned().unwrap_or_default();
    for (channel, hours) in cadence {
        if channel == "youtube" && s.youtube.refresh_token.is_empty() {
            continue;
        }
        let fmt = crate::default_format(&channel);
        let mut ready: Vec<&Render> = renders.iter().filter(|r| r.format == fmt && !posts.iter().any(|p| p.channel == channel && p.candidate_id.as_deref() == Some(&r.candidate_id))).collect();
        ready.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        let hours: Vec<i64> = hours.as_array().map(|h| h.iter().filter_map(|v| v.as_i64()).collect()).unwrap_or_default();
        let mut slots = Vec::new();
        for d in 1..=days {
            for h in &hours {
                let t = chrono::Local::now().date_naive() + chrono::Duration::days(d);
                if let Some(dt) = t.and_hms_opt(*h as u32, 0, 0).and_then(|n| n.and_local_timezone(chrono::Local).single()) {
                    let at = dt.with_timezone(&chrono::Utc).to_rfc3339();
                    if !posts.iter().any(|p| p.channel == channel && p.scheduled_at[..13] == at[..13]) {
                        slots.push(at);
                    }
                }
            }
        }
        for (r, at) in ready.iter().zip(slots) {
            let Some(c) = lib.get::<Candidate>("candidates", &r.candidate_id) else { continue };
            let src = lib.get::<Source>("sources", &c.source_id);
            let mut p = Post { id: new_id("p"), render_id: Some(r.id.clone()), candidate_id: Some(c.id.clone()), poster_id: None, channel: channel.clone(), scheduled_at: at, status: "planned".into(), title: c.title.clone(), caption: format!("{}\n{}", c.caption, c.hashtags.iter().map(|h| format!("#{h}")).collect::<Vec<_>>().join(" ")), created_at: now(), ..Default::default() };
            if let Some(src) = src {
                p.caption = p.caption.replace("{source_url}", src.url.as_deref().unwrap_or(""));
            }
            lib.save_post(&mut p);
            n += 1;
        }
    }
    n
}

/// Three closing cards (9:16, 4:5, 16:9) in the poster's style, then sized exactly.
fn endcards(lib: &Library, job: &Job) -> Result<Value, String> {
    let mut p: Poster = lib.get("posters", &job.ref_id).ok_or("poster is gone")?;
    lib.job_progress(&job.id, 0.05, "Claude + Higgsfield are drawing the end cards");
    let files = posters::generate_end_cards(lib, &p, |pid| lib.register_child(&job.id, pid))?;
    lib.job_progress(&job.id, 0.85, "sizing");
    let dir = PathBuf::from(&p.folder);
    let mut out = json!({});
    for (key, w, h) in [("9x16", 1080, 1920), ("4x5", 1080, 1350), ("16x9", 1920, 1080)] {
        let Some(src) = files.get(key) else { continue };
        let dst = dir.join(format!("endcard-{key}.png"));
        ffmpeg::poster_pad(src, &dst, w, h)?;
        out[format!("endcard-{key}")] = json!(dst);
    }
    if out.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err("no end cards came back".into());
    }
    if !p.outputs.is_object() {
        p.outputs = json!({});
    }
    crate::merge(&mut p.outputs, &out);
    lib.save_poster(&mut p);
    Ok(json!({ "message": format!("{} end cards", out.as_object().map(|o| o.len()).unwrap_or(0)) }))
}

/// Word-for-word corrections of clear transcript errors by the brain; keeps timings.
fn polish(lib: &Library, job: &Job) -> Result<Value, String> {
    let src = source(lib, &job.ref_id)?;
    let mut t: Transcript = lib.get("transcripts", &src.id).ok_or("transcribe first")?;
    let s = lib.settings();
    let chunks = brain::chunks(&t.segments);
    let mut fixes = Vec::new();
    for (n, (a, b)) in chunks.iter().enumerate() {
        lib.job_progress(&job.id, n as f64 / chunks.len() as f64, &format!("{} proofreading part {} of {}", s.brain, n + 1, chunks.len()));
        let prompt = brain::polish_prompt(&t.segments[*a..*b], *a, &t.language);
        let text = brain::ask(&s, &prompt, Duration::from_secs(10 * 60), |pid| lib.register_child(&job.id, pid))?;
        match brain::parse_fixes(&text) {
            Ok(f) => fixes.extend(f),
            Err(e) => lib.job_log(&job.id, &format!("{e}: {}", text.chars().take(300).collect::<String>())),
        }
    }
    let total_words: usize = t.segments.iter().map(|g| g.text.split_whitespace().count()).sum();
    let changed: usize = fixes.iter().map(|f| f.from.split_whitespace().count()).sum();
    // Faithfulness guard: more than 5% of the words touched is not proofreading any more.
    if total_words > 0 && changed * 20 > total_words {
        lib.job_log(&job.id, &format!("{changed} of {total_words} words would change; keeping the transcript as heard"));
        fixes.clear();
    }
    let applied = brain::apply_fixes(&mut t.segments, &fixes);
    if applied > 0 {
        let text: String = t.segments.iter().map(|g| g.text.as_str()).collect::<Vec<_>>().join(" ");
        lib.put("transcripts", &src.id, &now(), &t);
        let _ = lib.db.lock().fts_put(&src.id, &text);
        let _ = std::fs::write(Path::new(&src.folder).join("transcript.srt"), captions::srt(&t.segments, 0.0, f64::MAX));
        for f in &fixes {
            lib.job_log(&job.id, &format!("#{} {} → {}", f.seg, f.from, f.to));
        }
        lib.changed();
    }
    if job.args["thenDetect"].as_bool().unwrap_or(false) {
        lib.enqueue("detect", &src.id, &src.title, json!({}));
    }
    Ok(json!({ "message": format!("{applied} fixes in {total_words} words") }))
}
