//! Everything that goes through ffmpeg/ffprobe: probing, the reel render
//! (crop + scale + burned captions + loudness), covers, the waiting loop.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

#[derive(Debug, Clone, Copy, Default)]
pub struct Probe {
    pub duration: f64,
    pub width: i64,
    pub height: i64,
}

pub fn probe(path: &Path) -> Result<Probe, String> {
    let out = Command::new(crate::tools::ffprobe_bin()).args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path).output().map_err(|e| format!("ffprobe: {e}"))?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe json: {e}"))?;
    let video = v["streams"].as_array().into_iter().flatten().find(|s| s["codec_type"] == "video").cloned().unwrap_or(Value::Null);
    Ok(Probe {
        duration: v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0),
        width: video["width"].as_i64().unwrap_or(1920),
        height: video["height"].as_i64().unwrap_or(1080),
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
    let target = tw as f64 / th as f64;
    let source = sw as f64 / sh as f64;
    let (cw, ch) = if source > target { ((sh as f64 * target).round() as i64, sh) } else { (sw, (sw as f64 / target).round() as i64) };
    let cw = (cw / 2) * 2;
    let ch = (ch / 2) * 2;
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
    pub ass: Option<&'a Path>,
}

/// H.264 through VideoToolbox, AAC 192k, loudness −14 LUFS. Progress from
/// `-progress pipe:1`. Falls back to libx264 if the hardware encoder refuses.
pub fn render(r: &RenderSpec, mut on_progress: impl FnMut(f64, &str), on_spawn: impl FnOnce(u32)) -> Result<(), String> {
    let dur = (r.end - r.start).max(0.1);
    let mut vf = vec![crop_filter(r.src_w, r.src_h, r.width, r.height, r.crop_x, r.crop_y), format!("scale={}:{}:flags=lanczos", r.width, r.height)];
    if let Some(a) = r.ass {
        // libass wants the path escaped for the filter graph: ':' and '\' and quotes.
        vf.push(format!("ass='{}'", ass_path_arg(a)));
    }
    let vf = vf.join(",");
    let mut run = |encoder: &[&str]| -> Result<(), String> {
        let mut cmd = Command::new(crate::tools::ffmpeg_bin());
        cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error", "-ss", &format!("{:.3}", r.start), "-to", &format!("{:.3}", r.end)])
            .arg("-i")
            .arg(r.src)
            .args(["-vf", &vf])
            .args(encoder)
            .args(["-pix_fmt", "yuv420p", "-r", "30", "-c:a", "aac", "-b:a", "192k", "-af", "loudnorm=I=-14:TP=-1.5:LRA=11", "-movflags", "+faststart", "-progress", "pipe:1"])
            .arg(r.out)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        let pid = child.id();
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
                        on_progress((t / dur).min(0.99), &format!("{} of {}", crate::model::fmt_time(t), crate::model::fmt_time(dur)));
                    }
                }
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        let err = err_thread.join().unwrap_or_default();
        let _ = pid;
        if status.success() && r.out.exists() {
            Ok(())
        } else {
            Err(format!("ffmpeg exit {status}: {}", err.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ")))
        }
    };
    let _ = &on_spawn;
    match run(&["-c:v", "h264_videotoolbox", "-b:v", "9M", "-maxrate", "12M", "-profile:v", "high", "-allow_sw", "1"]) {
        Ok(()) => Ok(()),
        Err(e) if e.contains("videotoolbox") || e.contains("Unknown encoder") => {
            log::warn!("videotoolbox failed ({e}); libx264 instead");
            run(&["-c:v", "libx264", "-preset", "medium", "-crf", "18"])
        }
        Err(e) => Err(e),
    }
}

/// One frame at `t`, cropped like the reel, as JPEG.
pub fn cover(src: &Path, t: f64, out: &Path, src_w: i64, src_h: i64, width: i64, height: i64, cx: f64, cy: f64) -> Result<(), String> {
    let vf = format!("{},scale={}:{}", crop_filter(src_w, src_h, width, height, cx, cy), width, height);
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error", "-ss", &format!("{t:.3}")]).arg("-i").arg(src).args(["-frames:v", "1", "-vf", &vf, "-q:v", "3"]).arg(out)).map(|_| ())
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
        let lines = vec!["Begint zo".to_string(), "Tafsir".to_string(), "vrijdag".to_string()];
        let a = waiting_ass(&WaitingSpec { poster: Path::new("p.png"), out: Path::new("o.mp4"), width: 1920, height: 1080, lines: &lines, countdown_from: Some(3725), loop_s: 3, font: "Helvetica" }, true);
        assert!(a.contains("Count,,0,0,0,,1:02:05") && a.contains("Count,,0,0,0,,1:02:03") && a.contains("Title,,0,0,0,,Tafsir"));
        assert_eq!(a.matches("Dialogue: 1,").count(), 3);
    }
}
