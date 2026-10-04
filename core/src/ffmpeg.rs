//! Everything that goes through ffmpeg/ffprobe: probing, the reel render
//! (crop + scale + burned captions + loudness), covers, the waiting loop.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

/// Caption fonts that ship with the app (SIL OFL), shared with the webview's @font-face in app.css.
const FONTS: &[(&str, &[u8])] = &[
    ("Montserrat-ExtraBold.ttf", include_bytes!("../../desktop/src/fonts/Montserrat-ExtraBold.ttf")),
    ("Poppins-ExtraBold.ttf", include_bytes!("../../desktop/src/fonts/Poppins-ExtraBold.ttf")),
    ("Anton-Regular.ttf", include_bytes!("../../desktop/src/fonts/Anton-Regular.ttf")),
    ("BebasNeue-Regular.ttf", include_bytes!("../../desktop/src/fonts/BebasNeue-Regular.ttf")),
    ("ArchivoBlack-Regular.ttf", include_bytes!("../../desktop/src/fonts/ArchivoBlack-Regular.ttf")),
    ("LilitaOne-Regular.ttf", include_bytes!("../../desktop/src/fonts/LilitaOne-Regular.ttf")),
];

static FONTS_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Writes the bundled fonts where libass finds them (every `ass=` filter passes `fontsdir`).
pub fn install_fonts(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    for (name, bytes) in FONTS {
        let p = dir.join(name);
        if std::fs::metadata(&p).map(|m| m.len() != bytes.len() as u64).unwrap_or(true) {
            let _ = std::fs::write(&p, bytes);
        }
    }
    let _ = FONTS_DIR.set(dir.to_path_buf());
}

/// The libass filter for an .ass file, with the bundled fonts.
pub fn ass_filter(p: &Path) -> String {
    match FONTS_DIR.get() {
        Some(d) => format!("ass='{}':fontsdir='{}'", ass_path_arg(p), ass_path_arg(d)),
        None => format!("ass='{}'", ass_path_arg(p)),
    }
}

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

/// Integrated loudness (EBU R128, LUFS) of a file's audio; None when it has none or ffmpeg fails.
pub fn lufs(path: &Path) -> Option<f64> {
    lufs_with(path, |_| {})
}

/// Same, handing the ffmpeg pid to `on_spawn` so a job can kill a long measurement.
pub fn lufs_with(path: &Path, on_spawn: impl FnOnce(u32)) -> Option<f64> {
    let child = Command::new(crate::tools::ffmpeg_bin()).args(["-hide_banner", "-nostats", "-vn"]).arg("-i").arg(path).args(["-af", "ebur128", "-f", "null", "-"]).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().ok()?;
    on_spawn(child.id());
    let out = child.wait_with_output().ok()?;
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines().rev().find_map(|l| l.trim().strip_prefix("I:")).and_then(|v| v.trim().trim_end_matches("LUFS").trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v > -70.0)
}

/// The amplitude for background music: `level` against the speech (1 = as loud as the speaker,
/// 0.1 = 20 dB under), corrected by how loud the track and the speech really are. Unknown loudness
/// on either side leaves the level as a plain gain.
pub fn music_gain(level: f64, speech_lufs: Option<f64>, track_lufs: Option<f64>) -> f64 {
    let fit = match (speech_lufs, track_lufs) {
        (Some(s), Some(t)) => 10f64.powf((s - t) / 20.0),
        _ => 1.0,
    };
    (level.max(0.0) * fit).min(8.0)
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
    /// The punchline played first, then the transition into the cut.
    pub intro: Option<Intro<'a>>,
    pub logo: Option<LogoSpec<'a>>,
    /// A still appended after the cut, for this many seconds, with a short fade.
    pub end_card: Option<(&'a Path, f64)>,
    /// Background track (looped to length) and its level (0..1); it ducks under speech.
    pub music: Option<(&'a Path, f64)>,
    /// Seconds the last frame holds (fading to black) after the reel so the music can play on.
    pub tail: f64,
    /// The music plays on under the end card (else it fades out where the card begins).
    pub music_on_card: bool,
    /// Loudness of the finished file, LUFS.
    pub loudness: f64,
    /// The source has an audio stream (a screen recording may not).
    pub has_audio: bool,
    /// Output frame rate: the source's, capped at 60.
    pub fps: f64,
    /// "best" (x264 slow, crf 16) | "good" (x264 medium, crf 18) | "fast" (VideoToolbox 12 Mbps)
    pub quality: &'a str,
}

pub struct Intro<'a> {
    pub start: f64,
    pub end: f64,
    pub ass: Option<&'a Path>,
    pub transition: &'a str,
}

pub struct LogoSpec<'a> {
    pub path: &'a Path,
    /// Place within the free space: 0 = left/top edge, 1 = right/bottom edge.
    pub x: f64,
    pub y: f64,
    /// Width as a fraction of the frame width.
    pub size: f64,
    pub opacity: f64,
    /// Over the end card too (else it stops where the card starts).
    pub on_card: bool,
}

fn logo_width(l: &LogoSpec, w: i64) -> i64 {
    (((w as f64 * l.size.clamp(0.03, 0.8)) / 2.0).round() as i64 * 2).max(2)
}

/// The logo's filters over the stream `v`, the logo image being input `lk`; returns them and the new label.
/// `until`: only over the first seconds (a stamp on a finished file that ends in an end card).
fn logo_filters(l: &LogoSpec, lk: usize, w: i64, v: &str, until: Option<f64>) -> (Vec<String>, String) {
    let lw = logo_width(l, w);
    let out = format!("{v}l");
    let enable = until.map(|u| format!(":enable='lt(t,{u:.3})'")).unwrap_or_default();
    (
        vec![
            format!("[{lk}:v]scale={lw}:-2,format=rgba,colorchannelmixer=aa={:.2}[lg]", l.opacity.clamp(0.05, 1.0)),
            format!("[{v}][lg]overlay=x='(W-w)*{:.4}':y='(H-h)*{:.4}'{enable},format=yuv420p[{out}]", l.x.clamp(0.0, 1.0), l.y.clamp(0.0, 1.0)),
        ],
        out,
    )
}

/// Does the finished `file` already carry this logo at `t`? Laying it on again barely changes the
/// logo's box when it is there (PSNR ≈ ∞ at full opacity), and changes it a lot when it is not (~20 dB).
pub fn has_logo(file: &Path, l: &LogoSpec, t: f64) -> Result<bool, String> {
    let pr = probe(file)?;
    let lp = probe(l.path)?;
    let lw = logo_width(l, pr.width);
    let lh = ((lw as f64 * lp.height as f64 / lp.width.max(1) as f64 / 2.0).round() as i64 * 2).max(2);
    let (x, y) = (((pr.width - lw) as f64 * l.x.clamp(0.0, 1.0)) as i64, ((pr.height - lh) as f64 * l.y.clamp(0.0, 1.0)) as i64);
    let (f, out) = logo_filters(l, 1, pr.width, "b", None);
    let fc = format!("[0:v]split[a][b];{};[a]crop={lw}:{lh}:{x}:{y}[a2];[{out}]crop={lw}:{lh}:{x}:{y}[c2];[a2][c2]psnr", f.join(";"));
    let o = Command::new(crate::tools::ffmpeg_bin()).args(["-hide_banner", "-nostats", "-ss", &format!("{t:.3}")]).arg("-i").arg(file).arg("-i").arg(l.path).args(["-frames:v", "1", "-filter_complex", &fc, "-f", "null", "-"]).output().map_err(|e| e.to_string())?;
    let err = String::from_utf8_lossy(&o.stderr);
    let avg = err.split("average:").nth(1).and_then(|s| s.split_whitespace().next()).ok_or("no PSNR from ffmpeg")?;
    // ponytail: one frame, fixed 35 dB line; sample more frames if a busy background ever fools it.
    Ok(avg == "inf" || avg.parse::<f64>().map(|p| p > 35.0).unwrap_or(false))
}

/// Burns the logo onto a finished file (over the first `until` seconds, else all of it); the sound is copied.
pub fn stamp_logo(file: &Path, out: &Path, l: &LogoSpec, until: Option<f64>, quality: &str, mut on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let pr = probe(file)?;
    let (f, v) = logo_filters(l, 1, pr.width, "0:v", until);
    let run = |encoder: &[&str], on_progress: &mut dyn FnMut(f64, &str)| {
        let mut cmd = Command::new(crate::tools::ffmpeg_bin());
        cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error"]).arg("-i").arg(file).arg("-i").arg(l.path);
        cmd.args(["-filter_complex", &f.join(";"), "-map", &format!("[{v}]"), "-map", "0:a?"]).args(encoder).args(["-pix_fmt", "yuv420p", "-c:a", "copy", "-movflags", "+faststart", "-progress", "pipe:1"]).arg(out);
        run_with_progress(&mut cmd, pr.duration.max(0.1), out, on_progress, &mut |_| {})
    };
    match run(&encoder_args(quality), &mut on_progress) {
        Err(e) if e.contains("videotoolbox") || e.contains("Unknown encoder") => run(&encoder_args("good"), &mut on_progress),
        r => r,
    }
}

/// The teaser→reel transitions, picture only (the music carries the sound): (xfade name or None for a
/// hard cut, seconds).
pub const TRANSITIONS: &[(&str, Option<&str>, f64)] = &[
    ("swoosh", Some("smoothleft"), 0.35),
    ("zoom", Some("zoomin"), 0.4),
    ("slide", Some("slideup"), 0.3),
    ("blur", Some("hblur"), 0.4),
    ("flash", Some("fadewhite"), 0.3),
    ("fade", Some("fade"), 0.4),
    ("cut", None, 0.0),
];

pub fn transition(name: &str) -> (Option<&'static str>, f64) {
    let t = TRANSITIONS.iter().find(|t| t.0 == name).unwrap_or(&TRANSITIONS[0]);
    (t.1, t.2)
}

/// Length of the finished file: teaser + cut − the overlap of the transition + end card.
pub fn total_seconds(start: f64, end: f64, intro: Option<(f64, f64, &str)>, card: f64) -> f64 {
    let main = end - start;
    let lead = intro.map(|(a, b, t)| (b - a) - transition(t).1).unwrap_or(0.0);
    main + lead + card
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
fn run_with_progress(cmd: &mut Command, total: f64, out: &Path, mut on_progress: impl FnMut(f64, &str), on_spawn: &mut dyn FnMut(u32)) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    on_spawn(child.id());
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
    run_with_progress(&mut cmd, duration.max(0.1), out, on_progress, &mut |_| {})
}

/// A 1080p H.264/AAC copy of a source the in-app player cannot play (VP9, AV1, 4K, Opus).
/// Renders still read the original.
pub fn proxy(src: &Path, out: &Path, duration: f64, on_progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let mut cmd = Command::new(crate::tools::ffmpeg_bin());
    cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error"]).arg("-i").arg(src);
    cmd.args(["-vf", "scale=trunc(iw*min(1\\,min(1920/iw\\,1920/ih))/2)*2:-2", "-c:v", "h264_videotoolbox", "-b:v", "8M", "-allow_sw", "1", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", "-progress", "pipe:1"]).arg(out);
    run_with_progress(&mut cmd, duration.max(0.1), out, on_progress, &mut |_| {})
}

/// H.264 at the chosen quality, AAC 192k, normalised to `loudness` LUFS. One filter graph: the teaser
/// and the cut (each with its own captions), the transition, the logo over both, the end
/// card, then the background music under all of it. Progress from `-progress pipe:1`. Falls back to libx264 if the hardware encoder refuses.
pub fn render(r: &RenderSpec, mut on_progress: impl FnMut(f64, &str), mut on_spawn: impl FnMut(u32)) -> Result<(), String> {
    let dur = (r.end - r.start).max(0.1);
    let fps = if r.fps > 1.0 { r.fps.min(60.0) } else { 30.0 };
    let fps_s = format!("{fps:.3}");
    let (w, h) = (r.width, r.height);
    let base = format!("{},scale={w}:{h}:flags=lanczos,setsar=1,fps={fps_s},format=yuv420p,settb=AVTB", crop_filter_z(r.src_w, r.src_h, w, h, r.crop_x, r.crop_y, r.crop_z));
    let with_ass = |a: Option<&Path>| match a { Some(a) => format!("{base},{}", ass_filter(a)), None => base.clone() };
    let pcm = "aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo";
    let (tx, tx_d) = r.intro.as_ref().map(|i| transition(i.transition)).unwrap_or((None, 0.0));
    let card_s = r.end_card.map(|(_, s)| s).unwrap_or(0.0);
    let total = total_seconds(r.start, r.end, r.intro.as_ref().map(|i| (i.start, i.end, i.transition)), card_s) + r.tail;
    let mut run = |encoder: &[&str]| -> Result<(), String> {
        let mut cmd = Command::new(crate::tools::ffmpeg_bin());
        cmd.args(["-y", "-hide_banner", "-nostats", "-loglevel", "error"]);
        let mut n = 0usize;
        let mut input = |cmd: &mut Command, args: Vec<String>| -> usize {
            cmd.args(args);
            n += 1;
            n - 1
        };
        let src = r.src.display().to_string();
        let silent = |secs: f64| vec!["-f".into(), "lavfi".into(), "-t".into(), format!("{secs:.3}"), "-i".into(), "anullsrc=r=48000:cl=stereo".into()];
        let mut fc: Vec<String> = Vec::new();
        // The cut, and its sound (a silent track stands in when the source has none).
        let m = input(&mut cmd, vec!["-ss".into(), format!("{:.3}", r.start), "-to".into(), format!("{:.3}", r.end), "-i".into(), src.clone()]);
        fc.push(format!("[{m}:v]{}[mv]", with_ass(r.ass)));
        let ma = if r.has_audio { m } else { input(&mut cmd, silent(dur)) };
        fc.push(format!("[{ma}:a]{pcm}[ma]"));
        let (mut v, mut a) = ("mv".to_string(), "ma".to_string());
        if let Some(i) = &r.intro {
            let p = (i.end - i.start).max(0.5);
            let k = input(&mut cmd, vec!["-ss".into(), format!("{:.3}", i.start), "-to".into(), format!("{:.3}", i.end), "-i".into(), src.clone()]);
            fc.push(format!("[{k}:v]{}[iv]", with_ass(i.ass)));
            let ka = if r.has_audio { k } else { input(&mut cmd, silent(p)) };
            fc.push(format!("[{ka}:a]{pcm}[ia]"));
            match tx {
                Some(name) => {
                    fc.push(format!("[iv][mv]xfade=transition={name}:duration={tx_d}:offset={:.3}[jv]", p - tx_d));
                    fc.push(format!("[ia][ma]acrossfade=d={tx_d}[ja]"));
                }
                None => fc.push("[iv][ia][mv][ma]concat=n=2:v=1:a=1[jv][ja]".into()),
            }
            (v, a) = ("jv".into(), "ja".into());
        }
        // The logo goes over the whole file when it stays on the end card, else over the reel only.
        let logo_last = r.end_card.is_some() && r.logo.as_ref().is_some_and(|l| l.on_card);
        if let Some(l) = r.logo.as_ref().filter(|_| !logo_last) {
            let lk = input(&mut cmd, vec!["-i".into(), l.path.display().to_string()]);
            let (f, out) = logo_filters(l, lk, w, &v, None);
            fc.extend(f);
            v = out;
        }
        if r.tail > 0.0 {
            // The picture holds and fades out; the speech track gets silence to match.
            let body = total_seconds(r.start, r.end, r.intro.as_ref().map(|i| (i.start, i.end, i.transition)), 0.0);
            fc.push(format!("[{v}]tpad=stop_mode=clone:stop_duration={t:.2},fade=t=out:st={body:.3}:d={t:.2}[tv]", t = r.tail));
            fc.push(format!("[{a}]apad=pad_dur={:.2}[ta]", r.tail));
            (v, a) = ("tv".into(), "ta".into());
        }
        if let Some((card, secs)) = r.end_card {
            let ck = input(&mut cmd, vec!["-loop".into(), "1".into(), "-framerate".into(), fps_s.clone(), "-t".into(), format!("{secs:.2}"), "-i".into(), card.display().to_string()]);
            let ca = input(&mut cmd, silent(secs));
            fc.push(format!("[{ck}:v]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1,fps={fps_s},format=yuv420p,settb=AVTB,fade=t=in:st=0:d=0.35[cv]"));
            fc.push(format!("[{v}][{a}][cv][{ca}:a]concat=n=2:v=1:a=1[fv][fa]"));
            (v, a) = ("fv".into(), "fa".into());
        }
        if let Some(l) = r.logo.as_ref().filter(|_| logo_last) {
            let lk = input(&mut cmd, vec!["-i".into(), l.path.display().to_string()]);
            let (f, out) = logo_filters(l, lk, w, &v, None);
            fc.extend(f);
            v = out;
        }
        if let Some((track, vol)) = r.music {
            // Looped to the full length, faded at both ends, pushed down while someone speaks.
            let len = if r.music_on_card { total } else { total - card_s };
            let mk = input(&mut cmd, vec!["-stream_loop".into(), "-1".into(), "-t".into(), format!("{len:.3}"), "-i".into(), track.display().to_string()]);
            fc.push(format!("[{a}]asplit=2[sp][sc]"));
            fc.push(format!("[{mk}:a]{pcm},volume={:.4},afade=t=in:d=0.6,afade=t=out:st={:.3}:d=1.2[mu]", vol.clamp(0.0, 8.0), (len - 1.2).max(0.0)));
            fc.push("[mu][sc]sidechaincompress=threshold=0.03:ratio=5:attack=40:release=600[duck]".into());
            fc.push("[sp][duck]amix=inputs=2:duration=first:normalize=0[mx]".into());
            a = "mx".into();
        }
        if r.has_audio || r.music.is_some() {
            fc.push(format!("[{a}]loudnorm=I={:.1}:TP=-1.0:LRA=11,{pcm}[na]", r.loudness.clamp(-24.0, -6.0)));
            a = "na".into();
        }
        cmd.args(["-filter_complex", &fc.join(";"), "-map", &format!("[{v}]"), "-map", &format!("[{a}]")]);
        cmd.args(encoder).args(["-pix_fmt", "yuv420p", "-r", &fps_s, "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-progress", "pipe:1"]).arg(r.out);
        // The pid goes to the job, so a cancel (or a newer render of the same file) kills this encode.
        run_with_progress(&mut cmd, total, r.out, &mut on_progress, &mut on_spawn)
    };
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
        vf.push(',');
        vf.push_str(&ass_filter(a));
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
        "[1:v]scale=-2:{poster_h}[fg];[0:v][fg]overlay={}:'{}+6*sin(2*PI*t/12)':shortest=1,{}",
        if landscape { "80".to_string() } else { "(W-w)/2".to_string() },
        if landscape { 60 } else { 40 },
        ass_filter(&ass)
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
        "[0:v]scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},boxblur=30:5,eq=brightness=-0.25[bg];[0:v]scale=-2:{poster_h}[fg];[bg][fg]overlay={}:{},{}",
        if landscape { "80".to_string() } else { "(W-w)/2".to_string() },
        if landscape { 60 } else { 40 },
        ass_filter(&ass)
    );
    let r = crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error"]).arg("-i").arg(poster).args(["-filter_complex", &filter, "-frames:v", "1"]).arg(out)).map(|_| ());
    let _ = std::fs::remove_file(&ass);
    r
}

/// A picture centred on a flat colour (`#RRGGBB`), fitted inside 80 % of the frame; transparency shows the colour.
pub fn card_on_color(image: &Path, out: &Path, width: i64, height: i64, color: &str) -> Result<(), String> {
    let hex = color.trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("not a colour: {color}"));
    }
    let (fw, fh) = (width * 4 / 5, height * 4 / 5);
    let filter = format!("color=c=0x{hex}:s={width}x{height}[bg];[0:v]scale={fw}:{fh}:force_original_aspect_ratio=decrease[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,format=rgb24");
    crate::tools::run(Command::new(crate::tools::ffmpeg_bin()).args(["-y", "-hide_banner", "-loglevel", "error"]).arg("-i").arg(image).args(["-filter_complex", &filter, "-frames:v", "1"]).arg(out)).map(|_| ())
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

    /// Teaser + swoosh + cut + logo + end card through real ffmpeg: the file is as long as planned.
    #[test]
    fn teaser_render() {
        let d = tempfile::tempdir().unwrap();
        let ff = crate::tools::ffmpeg_bin();
        let src = d.path().join("src.mp4");
        let logo = d.path().join("logo.png");
        let card = d.path().join("card.png");
        let ok = |args: &[&str]| Command::new(&ff).args(["-y", "-loglevel", "error"]).args(args).status().map(|s| s.success()).unwrap_or(false);
        if !ok(&["-f", "lavfi", "-i", "testsrc2=s=640x360:r=30:d=12", "-f", "lavfi", "-i", "sine=f=440:d=12", "-c:v", "libx264", "-c:a", "aac", "-shortest", src.to_str().unwrap()]) {
            return; // no ffmpeg here
        }
        assert!(ok(&["-f", "lavfi", "-i", "color=red@0.5:s=200x80,format=rgba", "-frames:v", "1", logo.to_str().unwrap()]));
        assert!(ok(&["-f", "lavfi", "-i", "color=blue:s=1080x1920", "-frames:v", "1", card.to_str().unwrap()]));
        let out = d.path().join("out.mp4");
        let spec = RenderSpec { src: &src, start: 2.0, end: 6.0, out: &out, width: 1080, height: 1920, src_w: 640, src_h: 360, crop_x: 0.5, crop_y: 0.5, crop_z: 1.0, ass: None, intro: Some(Intro { start: 8.0, end: 10.0, ass: None, transition: "swoosh" }), logo: Some(LogoSpec { path: &logo, x: 1.0, y: 0.0, size: 0.2, opacity: 0.8, on_card: true }), end_card: Some((&card, 1.0)), music: Some((&src, 0.3)), tail: 0.0, music_on_card: false, loudness: -11.0, has_audio: true, fps: 30.0, quality: "fast" };
        render(&spec, |_, _| {}, |_| {}).unwrap();
        let p = probe(&out).unwrap();
        let want = total_seconds(2.0, 6.0, Some((8.0, 10.0, "swoosh")), 1.0);
        assert!((p.duration - want).abs() < 0.3, "{} vs {want}", p.duration);
        assert!(p.has_audio && p.width == 1080);
    }

    /// A finished file without the logo: found missing, stamped (sound kept), then found present.
    #[test]
    fn stamp_logo_once() {
        let d = tempfile::tempdir().unwrap();
        let ff = crate::tools::ffmpeg_bin();
        let (file, logo, out) = (d.path().join("reel.mp4"), d.path().join("logo.png"), d.path().join("stamped.mp4"));
        let ok = |args: &[&str]| Command::new(&ff).args(["-y", "-loglevel", "error"]).args(args).status().map(|s| s.success()).unwrap_or(false);
        if !ok(&["-f", "lavfi", "-i", "testsrc2=s=540x960:r=30:d=3", "-f", "lavfi", "-i", "sine=f=440:d=3", "-c:v", "libx264", "-c:a", "aac", "-shortest", file.to_str().unwrap()]) {
            return; // no ffmpeg here
        }
        assert!(ok(&["-f", "lavfi", "-i", "color=white:s=300x120,format=rgba,drawbox=x=20:y=20:w=260:h=80:color=red:t=fill", "-frames:v", "1", logo.to_str().unwrap()]));
        let l = LogoSpec { path: &logo, x: 0.1, y: 0.05, size: 0.25, opacity: 1.0, on_card: false };
        assert!(!has_logo(&file, &l, 1.5).unwrap());
        stamp_logo(&file, &out, &l, Some(2.0), "fast", |_, _| {}).unwrap();
        assert!(has_logo(&out, &l, 1.5).unwrap());
        assert!(!has_logo(&out, &l, 2.5).unwrap(), "the logo stops at `until`");
        assert!(probe(&out).unwrap().has_audio);
    }

    #[test]
    fn music_sits_under_the_speech() {
        // A -16 LUFS nasheed under -32 LUFS speech at 10 %: 16 dB down to match, 20 dB more under.
        let g = music_gain(0.1, Some(-32.0), Some(-16.0));
        assert!((20.0 * g.log10() - (-36.0)).abs() < 0.01);
        assert_eq!(music_gain(0.3, None, Some(-16.0)), 0.3);
        assert_eq!(music_gain(5.0, Some(0.0), Some(-60.0)), 8.0);
    }

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
