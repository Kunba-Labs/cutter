//! yt-dlp sidecar: metadata, the 1080p download with captions, channel listings.
//! Gotchas carried over from membox: `--dump-single-json` implies simulate,
//! `--sub-langs xx.*` requests every auto-translation and gets 429'd.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::model::Segment;

pub fn info(url: &str) -> Result<Value, String> {
    let out = Command::new("yt-dlp").args(["--dump-single-json", "--skip-download", "--no-warnings", "--no-playlist"]).arg(url).output().map_err(|e| format!("yt-dlp: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().rev().find(|l| l.starts_with('{')).ok_or_else(|| format!("yt-dlp exit {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))?;
    serde_json::from_str(line).map_err(|e| format!("yt-dlp json: {e}"))
}

pub struct Downloaded {
    pub video: PathBuf,
    pub thumb: Option<PathBuf>,
    pub captions: Option<PathBuf>,
}

/// The best stream YouTube has (any resolution, VP9/AV1 included; AAC audio when offered)
/// into `dir/source.mp4`, the poster, and the captions
/// (manual first, then the original-language auto track) as VTT. `on_progress`
/// gets 0..1 from yt-dlp's own percentage lines.
pub fn download(url: &str, dir: &Path, lang: Option<&str>, mut on_progress: impl FnMut(f64, &str), on_spawn: impl FnOnce(u32)) -> Result<Downloaded, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let langs = match lang {
        Some(l) => format!("{l},{l}-orig,{l}-{l}"),
        None => "-live_chat".to_string(), // manual subs in any language; auto subs asked for again once the language is known
    };
    let mut cmd = Command::new("yt-dlp");
    cmd.args([
        "-f", "bv*+ba[ext=m4a]/bv*+ba/b",
        "-S", "res,fps,hdr:12,br",
        "--merge-output-format", "mp4",
        "--no-playlist", "--no-warnings", "--newline", "--progress",
        "--write-thumbnail", "--convert-thumbnails", "jpg",
        "--write-subs", "--write-auto-subs", "--sub-langs", &langs, "--sub-format", "vtt",
        "-o", "source.%(ext)s", "-o", "thumbnail:thumb.%(ext)s",
        "--continue",
    ])
    .arg(url)
    .current_dir(dir)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("yt-dlp: {e}"))?;
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
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix("[download]") {
            if let Some(pct) = rest.trim().split('%').next().and_then(|p| p.trim().parse::<f64>().ok()) {
                on_progress(pct / 100.0, rest.trim());
            }
        } else if line.starts_with("[Merger]") || line.starts_with("[ExtractAudio]") {
            on_progress(0.99, "merging");
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    let video = dir.join("source.mp4");
    if !video.exists() {
        return Err(format!("yt-dlp exit {status}: {}", err.lines().last().unwrap_or("no output")));
    }
    Ok(Downloaded { video, thumb: Some(dir.join("thumb.jpg")).filter(|p| p.exists()), captions: best_captions(dir, lang) })
}

/// Only the auto captions, once the language is known (whisper told us).
pub fn fetch_auto_captions(url: &str, dir: &Path, lang: &str) -> Option<PathBuf> {
    let langs = format!("{lang},{lang}-orig,{lang}-{lang}");
    let _ = Command::new("yt-dlp")
        .args(["--skip-download", "--no-playlist", "--no-warnings", "--write-subs", "--write-auto-subs", "--sub-langs", &langs, "--sub-format", "vtt", "-o", "source.%(ext)s"])
        .arg(url)
        .current_dir(dir)
        .output();
    best_captions(dir, Some(lang))
}

fn best_captions(dir: &Path, lang: Option<&str>) -> Option<PathBuf> {
    let mut vtts: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "vtt").unwrap_or(false)).collect();
    if vtts.is_empty() {
        return None;
    }
    // source.<lang>.vtt beats source.<lang>-orig.vtt beats anything else; shortest name = manual.
    let rank = |p: &PathBuf| {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let mut r = 10;
        if let Some(l) = lang {
            if name == format!("source.{l}.vtt") { r = 0 } else if name == format!("source.{l}-orig.vtt") { r = 1 } else if name.starts_with(&format!("source.{l}")) { r = 2 }
        }
        (r, name.len())
    };
    vtts.sort_by_key(rank);
    vtts.into_iter().next()
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelVideo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub duration: Option<f64>,
    pub uploaded_at: Option<String>,
}

/// The newest `limit` uploads of a channel or playlist, listing only.
pub fn channel_videos(url: &str, limit: usize) -> Result<Vec<ChannelVideo>, String> {
    let url = if url.contains("youtube.com/@") && !url.contains("/videos") && !url.contains("/playlist") { format!("{}/videos", url.trim_end_matches('/')) } else { url.to_string() };
    let out = Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-single-json", "--no-warnings", "--playlist-end", &limit.to_string(), "--extractor-args", "youtubetab:approximate_date"])
        .arg(&url)
        .output()
        .map_err(|e| format!("yt-dlp: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().rev().find(|l| l.starts_with('{')).ok_or_else(|| format!("yt-dlp exit {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))?;
    let v: Value = serde_json::from_str(line).map_err(|e| format!("yt-dlp json: {e}"))?;
    Ok(v["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let id = e["id"].as_str()?.to_string();
            let uploaded_at = e["timestamp"].as_i64().and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|d| d.to_rfc3339()).or_else(|| e["upload_date"].as_str().map(|d| format!("{}-{}-{}T00:00:00Z", &d[..4], &d[4..6], &d[6..8])));
            Some(ChannelVideo { url: format!("https://www.youtube.com/watch?v={id}"), id, title: e["title"].as_str().unwrap_or("").to_string(), duration: e["duration"].as_f64(), uploaded_at })
        })
        .collect())
}

/// VTT cues → segments (cue-level timing, no words). Rolling auto-sub repeats
/// are dropped the way membox does it.
pub fn vtt_to_segments(vtt: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut cur: Option<(f64, f64)> = None;
    let mut last = String::new();
    for line in vtt.lines() {
        let line = line.trim();
        if let Some((a, b)) = line.split_once(" --> ") {
            cur = Some((parse_stamp(a), parse_stamp(b.split_whitespace().next().unwrap_or(b))));
            continue;
        }
        if line.is_empty() || line == "WEBVTT" || line.starts_with("Kind:") || line.starts_with("Language:") || line.starts_with("NOTE") {
            continue;
        }
        let text = strip_tags(line);
        if text.is_empty() || text == last || last.ends_with(&text) {
            continue;
        }
        if let Some((s, e)) = cur {
            // Auto subs repeat the previous line in the next cue; keep the part that is new.
            let new_text = if !last.is_empty() && text.starts_with(&last) { text[last.len()..].trim().to_string() } else { text.clone() };
            if !new_text.is_empty() {
                out.push(Segment { start: s, end: e, text: new_text, words: vec![] });
            }
        }
        last = text;
    }
    out
}

fn parse_stamp(t: &str) -> f64 {
    let core = t.trim();
    let (hms, ms) = core.split_once('.').unwrap_or((core, "0"));
    let parts: Vec<f64> = hms.split(':').filter_map(|p| p.parse().ok()).collect();
    let secs = match parts.as_slice() {
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        [m, s] => m * 60.0 + s,
        [s] => *s,
        _ => 0.0,
    };
    secs + format!("0.{ms}").parse::<f64>().unwrap_or(0.0)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vtt_segments() {
        let vtt = "WEBVTT\nKind: captions\n\n00:00:01.000 --> 00:00:03.000\nhello<00:00:02.000><c> world</c>\n\n00:00:03.000 --> 00:00:05.000\nhello world\n\n00:01:05.500 --> 00:01:07.000\nnext line\n";
        let s = vtt_to_segments(vtt);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].text, "hello world");
        assert!((s[1].start - 65.5).abs() < 1e-6);
    }
}
