//! Everything that goes through ffmpeg/ffprobe: probing, the reel render
//! (crop + scale + burned captions + loudness), covers, the waiting loop.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub duration: f64,
    pub width: i64,
    pub height: i64,
    pub has_audio: bool,
    /// Frames per second, 30 when unreadable.
    pub fps: f64,
    /// "h264" | "vp9" | "av1" | …
    pub vcodec: String,
    pub acodec: String,
    /// Whole-file bitrate in kbit/s (video + audio).
    pub kbps: i64,
}

/// What a finished file must be: the asked size, the asked length within a second, at least
/// 24 fps, a real bitrate, and audio when the source had it. Empty = fine.
pub fn quality_issues(p: &Probe, width: i64, height: i64, seconds: f64, want_audio: bool) -> Vec<String> {
    let mut v = Vec::new();
    if p.width != width || p.height != height {
        v.push(format!("{}×{} instead of {width}×{height}", p.width, p.height));
    }
    if (p.duration - seconds).abs() > 1.0 {
        v.push(format!("{:.1} s instead of {seconds:.1} s", p.duration));
    }
    if p.fps < 23.9 {
        v.push(format!("{:.0} fps", p.fps));
    }
    if p.kbps < 2500 {
        v.push(format!("{} kbps", p.kbps));
    }
    if want_audio && !p.has_audio {
        v.push("no audio".into());
    }
    v
}

/// Facts of a file for the UI: size, fps, codec, bitrate, length.
pub fn info_json(p: &Probe, path: &Path) -> serde_json::Value {
    let mb = std::fs::metadata(path).map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    serde_json::json!({ "width": p.width, "height": p.height, "fps": (p.fps * 100.0).round() / 100.0, "vcodec": p.vcodec, "acodec": p.acodec, "kbps": p.kbps, "seconds": (p.duration * 10.0).round() / 10.0, "mb": (mb * 10.0).round() / 10.0 })
}

/// "30000/1001" → 29.97
fn parse_rate(s: &str) -> Option<f64> {
    let (n, d) = s.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d > 0.0 && n > 0.0).then(|| n / d)
}

pub fn probe(path: &Path) -> Result<Probe, String> {
    let out = Command::new(crate::tools::ffprobe_bin()).args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path).output().map_err(|e| format!("ffprobe: {e}"))?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe json: {e}"))?;
    let video = v["streams"].as_array().into_iter().flatten().find(|s| s["codec_type"] == "video").cloned().unwrap_or(Value::Null);
    let audio = v["streams"].as_array().into_iter().flatten().find(|s| s["codec_type"] == "audio").cloned();
    Ok(Probe {
        duration: v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0),
        width: video["width"].as_i64().unwrap_or(1920),
        height: video["height"].as_i64().unwrap_or(1080),
        has_audio: audio.is_some(),
        fps: video["avg_frame_rate"].as_str().and_then(parse_rate).or_else(|| video["r_frame_rate"].as_str().and_then(parse_rate)).unwrap_or(30.0),
        vcodec: video["codec_name"].as_str().unwrap_or_default().to_string(),
        acodec: audio.as_ref().and_then(|a| a["codec_name"].as_str()).unwrap_or_default().to_string(),
        kbps: v["format"]["bit_rate"].as_str().and_then(|b| b.parse::<i64>().ok()).unwrap_or(0) / 1000,
    })
}

/// Output geometry per platform format. (width, height, extra bottom caption margin)
pub fn format_spec(format: &str) -> (i64, i64, f64) {
    match format {
        "tiktok" => (1080, 1920, 0.08),
        "feed" => (1080, 1350, 0.0),
        "landscape" | "16x9" => (1920, 1080, 0.0),
        _ => (1080, 1920, 0.0), // shorts, reels
    }
}

/// crop=w:h:x:y for a source of `sw`×`sh` to the target aspect, centred on
/// `cx` (0..1 across the width) and `cy` (0..1 down the height).
pub fn crop_filter(sw: i64, sh: i64, tw: i64, th: i64, cx: f64, cy: f64) -> String {
    crop_filter_z(sw, sh, tw, th, cx, cy, 1.0)
}

/// Same, zoomed in by `z` (1 = the largest window that fits, 2 = half of it).
pub fn crop_filter_z(sw: i64, sh: i64, tw: i64, th: i64, cx: f64, cy: f64, z: f64) -> String {
    let z = z.clamp(1.0, 4.0);
    let target = tw as f64 / th as f64;
    let source = sw as f64 / sh as f64;
    let (cw, ch) = if source > target { (sh as f64 * target, sh as f64) } else { (sw as f64, sw as f64 / target) };
    let cw = ((cw / z).round() as i64 / 2) * 2;
    let ch = ((ch / z).round() as i64 / 2) * 2;
    let x = ((cx * sw as f64) - cw as f64 / 2.0).round().clamp(0.0, (sw - cw) as f64) as i64;
    let y = ((cy * sh as f64) - ch as f64 / 2.0).round().clamp(0.0, (sh - ch) as f64) as i64;
    format!("crop={cw}:{ch}:{x}:{y}")
}

pub struct RenderSpec<'a> {
    pub src: &'a Path,
    pub start: f64,
    pub end: f64,
    pub out: &'a Path,
    pub width: i64,
    pub height: i64,
    pub src_w: i64,
    pub src_h: i64,
    pub crop_x: f64,
    pub crop_y: f64,
    pub crop_z: f64,
    pub ass: Option<&'a Path>,
    /// A still appended after the cut, for this many seconds, with a short fade.
    pub end_card: Option<(&'a Path, f64)>,
    /// The source has an audio stream (a screen recording may not).
    pub has_audio: bool,
    /// Output frame rate: the source's, capped at 60.
    pub fps: f64,
    /// "best" (x264 slow, crf 16) | "good" (x264 medium, crf 18) | "fast" (VideoToolbox 12 Mbps)
    pub quality: &'a str,
}

/// Encoder arguments for a quality setting.
pub fn encoder_args(quality: &str) -> Vec<&'static str> {
    match quality {
        "fast" => vec!["-c:v", "h264_videotoolbox", "-b:v", "12M", "-maxrate", "16M", "-profile:v", "high", "-allow_sw", "1"],
        "good" => vec!["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-profile:v", "high"],
        _ => vec!["-c:v", "libx264", "-preset", "slow", "-crf", "16", "-profile:v", "high"],
    }
}

/// Aspect key for the end-card set: "9x16" | "4x5" | "16x9".
pub fn aspect_key(width: i64, height: i64) -> &'static str {
    if width > height { "16x9" } else if (width as f64 / height as f64) > 0.7 { "4x5" } else { "9x16" }
}

/// Runs an ffmpeg command that was given `-progress pipe:1`, reporting 0..1 of `total` seconds.
fn run_with_progress(cmd: &mut Command, total: f64, out: &Path, mut on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        for l in BufReader::new(stderr).lines().map_while(Result::ok) {
            s.push_str(&l);
            s.push('\n');
        }
        s
    });
    let mut last = std::time::Instant::now();
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(us) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
            if let Ok(v) = us.trim().parse::<f64>() {
                if last.elapsed().as_millis() > 400 {
                    last = std::time::Instant::now();
                    let t = v / 1_000_000.0;
                    on_progress((t / total).min(0.99), &format!("{} of {}", crate::model::fmt_time(t), crate::model::fmt_time(total)));
                }
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    if status.success() && out.exists() {
        Ok(())
    } else {
        Err(format!("ffmpeg exit {status}: {}", err.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ")))
    }
}

/// The same streams in a real MP4 with the index up front (no re-encode). Camera and editor
/// .mov files keep the index after the media; the in-app player then stalls on a black frame.
pub fn faststart(src: &Path, out: &Path, duration: f64, on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let mut cmd = Command::new(crate::tools::ffmpeg_bin());
    cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error"]).arg("-i").arg(src).args(["-map", "0:v:0", "-map", "0:a?", "-c", "copy", "-movflags", "+faststart", "-progress", "pipe:1"]).arg(out);
    run_with_progress(&mut cmd, duration.max(0.1), out, on_progress)
}

/// A 1080p H.264/AAC copy of a source the in-app player cannot play (VP9, AV1, 4K, Opus).
/// Renders still read the original.
pub fn proxy(src: &Path, out: &Path, duration: f64, on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let mut cmd = Command::new(crate::tools::ffmpeg_bin());
    cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error"]).arg("-i").arg(src);
    cmd.args(["-vf", "scale=trunc(iw*min(1\\,min(1920/iw\\,1920/ih))/2)*2:-2", "-c:v", "h264_videotoolbox", "-b:v", "8M", "-allow_sw", "1", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", "-progress", "pipe:1"]).arg(out);
    run_with_progress(&mut cmd, duration.max(0.1), out, on_progress)
}

/// H.264 at the chosen quality, AAC 192k, loudness −14 LUFS. Progress from
/// `-progress pipe:1`. Falls back to libx264 if the hardware encoder refuses.
pub fn render(r: &RenderSpec, mut on_progress: impl FnMut(f64, &str), on_spawn: impl FnOnce(u32)) -> Result<(), String> {
    let dur = (r.end - r.start).max(0.1);
    let mut vf = vec![crop_filter_z(r.src_w, r.src_h, r.width, r.height, r.crop_x, r.crop_y, r.crop_z), format!("scale={}:{}:flags=lanczos", r.width, r.height)];
    if let Some(a) = r.ass {
        // libass wants the path escaped for the filter graph: ':' and '\' and quotes.
        vf.push(format!("ass='{}'", ass_path_arg(a)));
    }
    let vf = vf.join(",");
    let total = dur + r.end_card.map(|(_, s)| s).unwrap_or(0.0);
    let fps = if r.fps > 1.0 { r.fps.min(60.0) } else { 30.0 };
    let fps_s = format!("{fps:.3}");
    let mut run = |encoder: &[&str]| -> Result<(), String> {
        let mut cmd = Command::new(crate::tools::ffmpeg_bin());
        cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error", "-ss", &format!("{:.3}", r.start), "-to", &format!("{:.3}", r.end)]).arg("-i").arg(r.src);
        match r.end_card {
            Some((card, secs)) => {
                // The cut, then the card as a second clip (silent audio), joined with a fade.
                cmd.args(["-loop", "1", "-framerate", &fps_s, "-t", &format!("{secs:.2}")]).arg("-i").arg(card);
                cmd.args(["-f", "lavfi", "-t", &format!("{secs:.2}"), "-i", "anullsrc=r=48000:cl=stereo"]);
                // A silent source stands in for a video that has no audio stream.
                let a0 = if r.has_audio {
                    "[0:a]loudnorm=I=-14:TP=-1.5:LRA=11,aresample=48000[a0]".to_string()
                } else {
                    cmd.args(["-f", "lavfi", "-t", &format!("{dur:.3}"), "-i", "anullsrc=r=48000:cl=stereo"]);
                    "[3:a]anull[a0]".to_string()
                };
                let fc = format!(
                    "[0:v]{vf},setsar=1,fps={fps_s},format=yuv420p[v0];{a0};[1:v]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1,fps={fps_s},format=yuv420p,fade=t=in:st=0:d=0.35[v1];[v0][a0][v1][2:a]concat=n=2:v=1:a=1[v][a]",
                    w = r.width,
                    h = r.height
                );
                cmd.args(["-filter_complex", &fc, "-map", "[v]", "-map", "[a]"]);
            }
            None => {
                cmd.args(["-vf", &vf]);
                if r.has_audio {
                    cmd.args(["-af", "loudnorm=I=-14:TP=-1.5:LRA=11"]);
                }
            }
        }
        cmd.args(encoder).args(["-pix_fmt", "yuv420p", "-r", &fps_s, "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-progress", "pipe:1"]).arg(r.out);
        run_with_progress(&mut cmd, total, r.out, &mut on_progress)
    };
    let _ = &on_spawn;
    match run(&encoder_args(r.quality)) {
        Ok(()) => Ok(()),
        Err(e) if e.contains("videotoolbox") || e.contains("Unknown encoder") => {
            log::warn!("hardware encoder failed ({e}); libx264 instead");
            run(&encoder_args("good"))
        }
        Err(e) => Err(e),
    }
}

/// One frame at `t`, cropped like the reel, as JPEG.
/// One frame at `t`, cropped like the reel, lightly sharpened, with the title burned in when `ass` is given.
#[allow(clippy::too_many_arguments)]
pub fn cover(src: &Path, t: f64, out: &Path, src_w: i64, src_h: i64, width: i64, height: i64, cx: f64, cy: f64, z: f64, ass: Option<&Path>) -> Result<(), String> {
    let mut vf = format!("{},scale={}:{}:flags=lanczos,unsharp=5:5:0.6:5:5:0.0", crop_filter_z(src_w, src_h, width, height, cx, cy, z), width, height);
    if let Some(a) = ass {
        vf.push_str(&format!(",ass='{}'", ass_path_arg(a)));
    }
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error", "-ss", &format!("{t:.3}")]).arg("-i").arg(src).args(["-frames:v", "1", "-vf", &vf, "-q:v", "2"]).arg(out)).map(|_| ())
}

/// The sharpest frame in each of `n` equal windows of the cut (mean Sobel edge strength at 4 fps
/// over the reel's crop), so a blink or a motion-blurred frame never reaches the sheet.
#[allow(clippy::too_many_arguments)]
pub fn sharp_times(src: &Path, start: f64, end: f64, src_w: i64, src_h: i64, width: i64, height: i64, cx: f64, cy: f64, z: f64, n: usize) -> Result<Vec<f64>, String> {
    let span = (end - start).max(0.5);
    let pad = (span * 0.08).min(0.5);
    let (lo, hi) = (start + pad, end - pad);
    let vf = format!("{},scale=360:-2,fps=4,format=gray,sobel,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-", crop_filter_z(src_w, src_h, width, height, cx, cy, z));
    let out = Command::new(crate::tools::ffmpeg_bin()).args(["-hide_banner", "-nostats", "-loglevel", "error", "-ss", &format!("{lo:.3}"), "-to", &format!("{hi:.3}")]).arg("-i").arg(src).args(["-vf", &vf, "-f", "null", "-"]).output().map_err(|e| format!("ffmpeg: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut scored: Vec<(f64, f64)> = Vec::new();
    let mut t = None;
    for line in text.lines() {
        if let Some(p) = line.split("pts_time:").nth(1) {
            t = p.trim().parse::<f64>().ok();
        } else if let (Some(v), Some(tt)) = (line.strip_prefix("lavfi.signalstats.YAVG="), t) {
            if let Ok(v) = v.trim().parse::<f64>() {
                scored.push((lo + tt, v));
            }
        }
    }
    if scored.len() < n {
        return Err(format!("{} scored frames", scored.len()));
    }
    let w = (hi - lo) / n as f64;
    let times = (0..n)
        .map(|i| {
            let (a, b) = (lo + w * i as f64, lo + w * (i + 1) as f64);
            scored.iter().filter(|(t, _)| *t >= a - 1e-6 && *t < b).max_by(|x, y| x.1.total_cmp(&y.1)).map(|(t, _)| *t).unwrap_or(a + w / 2.0)
        })
        .collect();
    Ok(times)
}

/// Nine frames across the cut, cropped like the reel, tiled 3×3 and numbered, for the brain
/// to choose a cover from. Returns the sheet and the source time of each numbered frame.
#[allow(clippy::too_many_arguments)]
pub fn cover_sheet(src: &Path, start: f64, end: f64, dir: &Path, src_w: i64, src_h: i64, width: i64, height: i64, cx: f64, cy: f64, z: f64) -> Result<(std::path::PathBuf, Vec<f64>), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let n = 9usize;
    let span = (end - start).max(0.5);
    let pad = (span * 0.08).min(0.5);
    let times: Vec<f64> = sharp_times(src, start, end, src_w, src_h, width, height, cx, cy, z, n).unwrap_or_else(|e| {
        log::warn!("sharpness pass: {e}; even spacing instead");
        (0..n).map(|i| start + pad + (span - 2.0 * pad) * (i as f64 + 0.5) / n as f64).collect()
    });
    let (fw, gap) = (360i64, 8i64);
    let fh = ((fw as f64 * height as f64 / width as f64 / 2.0).round() as i64) * 2;
    let vf = format!("{},scale={fw}:{fh}", crop_filter_z(src_w, src_h, width, height, cx, cy, z));
    for (i, t) in times.iter().enumerate() {
        crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error", "-ss", &format!("{t:.3}")]).arg("-i").arg(src).args(["-frames:v", "1", "-vf", &vf, "-q:v", "3"]).arg(dir.join(format!("frame-{}.jpg", i + 1))))?;
    }
    let (sw, sh) = (3 * fw + 4 * gap, 3 * fh + 4 * gap);
    let mut ass = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {sw}\nPlayResY: {sh}\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Num,Helvetica,60,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,0,0,3,8,0,7,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    );
    for i in 0..n as i64 {
        let (col, row) = (i % 3, i / 3);
        ass.push_str(&format!("Dialogue: 0,0:00:00.00,0:00:10.00,Num,,0,0,0,,{{\\an7\\pos({},{})}}{}\n", gap + col * (fw + gap) + 12, gap + row * (fh + gap) + 10, i + 1));
    }
    let ass_path = dir.join("numbers.ass");
    std::fs::write(&ass_path, ass).map_err(|e| e.to_string())?;
    let sheet = dir.join("sheet.jpg");
    let vf = format!("tile=3x3:padding={gap}:margin={gap}:color=black,ass='{}'", ass_path_arg(&ass_path));
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error", "-start_number", "1"]).arg("-i").arg(dir.join("frame-%d.jpg")).args(["-frames:v", "1", "-vf", &vf, "-q:v", "3"]).arg(&sheet))?;
    Ok((sheet, times))
}

pub fn thumbnail(src: &Path, out: &Path) -> Result<(), String> {
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error", "-ss", "30"]).arg("-i").arg(src).args(["-frames:v", "1", "-vf", "scale=640:-2", "-q:v", "4"]).arg(out)).map(|_| ())
}

/// Silences (start, end) longer than `min_s`, for snapping cut points.
pub fn silences(src: &Path, min_s: f64) -> Vec<(f64, f64)> {
    let out = Command::new(crate::tools::ffmpeg_bin()).args(["-hide_banner", "-nostats"]).arg("-i").arg(src).args(["-af", &format!("silencedetect=n=-35dB:d={min_s}"), "-f", "null", "-"]).output();
    let Ok(out) = out else { return vec![] };
    let text = String::from_utf8_lossy(&out.stderr);
    let mut res = Vec::new();
    let mut start = None;
    for l in text.lines() {
        if let Some(p) = l.split("silence_start: ").nth(1) {
            start = p.trim().split_whitespace().next().and_then(|v| v.parse().ok());
        } else if let Some(p) = l.split("silence_end: ").nth(1) {
            if let (Some(s), Some(e)) = (start.take(), p.trim().split_whitespace().next().and_then(|v| v.parse::<f64>().ok())) {
                res.push((s, e));
            }
        }
    }
    res
}

pub struct WaitingSpec<'a> {
    pub poster: &'a Path,
    pub out: &'a Path,
    pub width: i64,
    pub height: i64,
    pub lines: &'a [String],
    /// Seconds until the event starts, at the moment the loop begins. None = no countdown.
    pub countdown_from: Option<i64>,
    pub loop_s: i64,
    pub font: &'a str,
}

fn ass_esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('{', "(").replace('}', ")")
}

fn ass_ts(t: i64) -> String {
    format!("{}:{:02}:{:02}.00", t / 3600, (t % 3600) / 60, t % 60)
}

/// The text of the waiting loop as an ASS file (this ffmpeg has libass but no
/// drawtext). Static lines for the whole loop; the countdown is one Dialogue
/// per second. `text_x`/`align` place it beside (landscape) or under (portrait) the poster.
fn waiting_ass(w: &WaitingSpec, landscape: bool) -> String {
    let (pw, ph) = (w.width, w.height);
    let font = w.font;
    // Landscape: left-aligned block starting at 44% width; portrait: centred under the poster.
    let (align, margin_l, top) = if landscape { (7, (pw as f64 * 0.44) as i64, ph / 2 - 210) } else { (8, 0, (ph as f64 * 0.62) as i64 + 60) };
    let mut s = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {pw}\nPlayResY: {ph}\nWrapStyle: 2\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Kicker,{font},34,&H00A6D8E9,&H00FFFFFF,&H00000000,&H00000000,-1,0,0,0,100,100,4,0,1,0,0,{align},{margin_l},40,{top},1\n\
Style: Title,{font},72,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,2,1,{align},{margin_l},40,{t2},1\n\
Style: Line,{font},34,&H00F0F0F0,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,1,1,{align},{margin_l},40,{t3},1\n\
Style: Count,{font},120,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,-1,0,0,0,100,100,-2,0,1,2,1,{align},{margin_l},40,{t4},1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        t2 = top + 50,
        t3 = top + 150,
        t4 = top + 150 + 44 * w.lines.len().saturating_sub(2) as i64 + 30,
    );
    let end = ass_ts(w.loop_s);
    for (i, line) in w.lines.iter().enumerate() {
        let (style, dy) = match i { 0 => ("Kicker", 0), 1 => ("Title", 0), n => ("Line", 44 * (n as i64 - 2)) };
        s.push_str(&format!("Dialogue: 0,0:00:00.00,{end},{style},,0,0,{},,{}\n", if dy > 0 { format!("{}", top + 150 + dy) } else { "0".into() }, ass_esc(line)));
    }
    if let Some(from) = w.countdown_from {
        for t in 0..w.loop_s {
            let rem = (from - t).max(0);
            let text = if rem >= 3600 { format!("{}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60) } else { format!("{}:{:02}", rem / 60, rem % 60) };
            s.push_str(&format!("Dialogue: 1,{},{},Count,,0,0,0,,{text}\n", ass_ts(t), ass_ts(t + 1)));
        }
    }
    s
}

fn ass_path_arg(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\").replace(':', "\\:").replace('\'', "\\'")
}

/// The blurred, darkened poster as a still background, rendered once so the
/// loop does not pay for a blur per frame.
fn background(poster: &Path, out: &Path, width: i64, height: i64) -> Result<(), String> {
    let vf = format!("scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},boxblur=30:5,eq=brightness=-0.25");
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error"]).arg("-i").arg(poster).args(["-vf", &vf, "-frames:v", "1"]).arg(out)).map(|_| ())
}

/// The "starting soon" loop: the poster floating slowly over its blurred
/// self, the text lines and a live countdown burned in through libass.
/// ponytail: a 12 s sine float instead of zoompan — hardware encode then runs
/// far faster than realtime; zoompan+boxblur per frame ran at ~5 fps.
pub fn waiting_video(w: &WaitingSpec) -> Result<(), String> {
    let landscape = w.width > w.height;
    let (pw, ph) = (w.width, w.height);
    let poster_h = if landscape { ph - 120 } else { (ph as f64 * 0.62) as i64 };
    let bg = w.out.with_extension("bg.png");
    background(w.poster, &bg, pw, ph)?;
    let ass = w.out.with_extension("ass");
    std::fs::write(&ass, waiting_ass(w, landscape)).map_err(|e| e.to_string())?;
    let filter = format!(
        "[1:v]scale=-2:{poster_h}[fg];[0:v][fg]overlay={}:'{}+6*sin(2*PI*t/12)':shortest=1,ass='{}'",
        if landscape { "80".to_string() } else { "(W-w)/2".to_string() },
        if landscape { 60 } else { 40 },
        ass_path_arg(&ass)
    );
    let r = crate::tools::run(
        Command::new(crate::tools::ffmpeg_bin())
            .args(["-y", "-hide_banner", "-loglevel", "error", "-loop", "1", "-framerate", "25"])
            .arg("-i")
            .arg(&bg)
            .args(["-loop", "1", "-framerate", "25"])
            .arg("-i")
            .arg(w.poster)
            .args(["-t", &w.loop_s.to_string(), "-filter_complex", &filter, "-c:v", "h264_videotoolbox", "-b:v", "6M", "-pix_fmt", "yuv420p", "-r", "25", "-an", "-movflags", "+faststart"])
            .arg(w.out),
    )
    .map(|_| ());
    let _ = std::fs::remove_file(&bg);
    r
}

/// A still card (break / ended) with the poster and one line of text.
pub fn still(poster: &Path, out: &Path, width: i64, height: i64, text: &str, font: &str) -> Result<(), String> {
    let landscape = width > height;
    let spec = WaitingSpec { poster, out, width, height, lines: &[String::new(), text.to_string()], countdown_from: None, loop_s: 1, font };
    let ass = out.with_extension("ass");
    std::fs::write(&ass, waiting_ass(&spec, landscape)).map_err(|e| e.to_string())?;
    let poster_h = if landscape { height - 120 } else { (height as f64 * 0.62) as i64 };
    let filter = format!(
        "[0:v]scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},boxblur=30:5,eq=brightness=-0.25[bg];[0:v]scale=-2:{poster_h}[fg];[bg][fg]overlay={}:{},ass='{}'",
        if landscape { "80".to_string() } else { "(W-w)/2".to_string() },
        if landscape { 60 } else { 40 },
        ass_path_arg(&ass)
    );
    let r = crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error"]).arg("-i").arg(poster).args(["-filter_complex", &filter, "-frames:v", "1"]).arg(out)).map(|_| ());
    let _ = std::fs::remove_file(&ass);
    r
}

/// Poster crops for feed 4:5 and story 9:16: the poster padded on a matching
/// blurred background, so nothing is cut off.
pub fn poster_pad(poster: &Path, out: &Path, width: i64, height: i64) -> Result<(), String> {
    let filter = format!("[0:v]scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},boxblur=40:8[bg];[0:v]scale={width}:{height}:force_original_aspect_ratio=decrease[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2");
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error"]).arg("-i").arg(poster).args(["-filter_complex", &filter, "-frames:v", "1"]).arg(out)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crops() {
        assert_eq!(crop_filter(1920, 1080, 1080, 1920, 0.5, 0.5), "crop=608:1080:656:0");
        assert_eq!(crop_filter(1920, 1080, 1080, 1920, 0.0, 0.5), "crop=608:1080:0:0");
        assert_eq!(crop_filter(1920, 1080, 1080, 1920, 1.0, 0.5), "crop=608:1080:1312:0");
        assert_eq!(crop_filter(1920, 1080, 1080, 1350, 0.5, 0.5), "crop=864:1080:528:0");
        assert_eq!(crop_filter(1080, 1920, 1920, 1080, 0.5, 0.5), "crop=1080:608:0:656");
        assert_eq!(format_spec("tiktok").2, 0.08);
        assert_eq!(crop_filter_z(1920, 1080, 1080, 1920, 0.5, 0.5, 2.0), "crop=304:540:808:270");
        assert_eq!(crop_filter_z(1920, 1080, 1080, 1920, 0.5, 0.0, 2.0), "crop=304:540:808:0");
        let lines = vec!["Begint zo".to_string(), "Tafsir".to_string(), "vrijdag".to_string()];
        let a = waiting_ass(&WaitingSpec { poster: Path::new("p.png"), out: Path::new("o.mp4"), width: 1920, height: 1080, lines: &lines, countdown_from: Some(3725), loop_s: 3, font: "Helvetica" }, true);
        assert!(a.contains("Count,,0,0,0,,1:02:05") && a.contains("Count,,0,0,0,,1:02:03") && a.contains("Title,,0,0,0,,Tafsir"));
        assert_eq!(a.matches("Dialogue: 1,").count(), 3);
    }
}
