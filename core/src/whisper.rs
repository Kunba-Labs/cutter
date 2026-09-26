//! Local transcription through the `mlx_whisper` CLI (uv tool). Word timestamps
//! on, language auto-detected unless told, the glossary as the initial prompt.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::model::{Segment, Word};

pub struct Transcribed {
    pub language: String,
    pub segments: Vec<Segment>,
}

pub fn transcribe(video: &Path, model: &str, language: Option<&str>, glossary: &[String], duration: f64, mut on_progress: impl FnMut(f64, &str), on_spawn: impl FnOnce(u32)) -> Result<Transcribed, String> {
    let dir = video.parent().ok_or("no dir")?;
    let mut cmd = Command::new("mlx_whisper");
    cmd.arg(video)
        .args(["--model", model, "--output-dir"])
        .arg(dir)
        .args(["--output-name", "transcript", "--output-format", "json", "--word-timestamps", "True", "--verbose", "True", "--condition-on-previous-text", "False"]);
    if let Some(l) = language {
        cmd.args(["--language", l]);
    }
    if !glossary.is_empty() {
        cmd.args(["--initial-prompt", &glossary.join(", ")]);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("mlx_whisper: {e}"))?;
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
    // Verbose mode prints "[00:12.340 --> 00:15.000]  text" per segment: progress comes from the end stamp.
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix('[') {
            if let Some((_, tail)) = rest.split_once(" --> ") {
                let stamp = tail.split(']').next().unwrap_or("");
                let secs = parse_stamp(stamp);
                if duration > 0.0 {
                    on_progress((secs / duration).min(0.99), &format!("{} of {}", crate::model::fmt_time(secs), crate::model::fmt_time(duration)));
                }
            }
        } else if line.starts_with("Detected language") {
            on_progress(0.01, line.trim());
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    let json_path = dir.join("transcript.json");
    if !status.success() || !json_path.exists() {
        let tail: Vec<&str> = err.lines().rev().take(8).collect();
        return Err(format!("mlx_whisper exit {status}: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | ")));
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&json_path).map_err(|e| e.to_string())?).map_err(|e| format!("transcript.json: {e}"))?;
    Ok(parse(&v))
}

pub fn parse(v: &Value) -> Transcribed {
    let language = v["language"].as_str().unwrap_or("").to_string();
    let segments = v["segments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let text = s["text"].as_str()?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            let words = s["words"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|w| Some(Word { w: w["word"].as_str()?.trim().to_string(), s: w["start"].as_f64()?, e: w["end"].as_f64()? }))
                .filter(|w| !w.w.is_empty())
                .collect();
            Some(Segment { start: s["start"].as_f64()?, end: s["end"].as_f64()?, text, words })
        })
        .collect();
    Transcribed { language, segments }
}

/// Whisper's own error text when the model does not fit; the caller retries smaller.
pub fn is_oom(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.contains("out of memory") || e.contains("insufficient memory") || e.contains("metal") && e.contains("alloc")
}

fn parse_stamp(t: &str) -> f64 {
    let parts: Vec<f64> = t.trim().split(':').filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        [m, s] => m * 60.0 + s,
        [s] => *s,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_words() {
        let v: Value = serde_json::from_str(r#"{"language":"nl","segments":[{"start":1.0,"end":2.5,"text":" sabr is ","words":[{"word":" sabr","start":1.0,"end":1.4},{"word":" is","start":1.5,"end":1.7}]}]}"#).unwrap();
        let t = parse(&v);
        assert_eq!(t.language, "nl");
        assert_eq!(t.segments[0].words[0].w, "sabr");
        assert!((parse_stamp("01:02.500") - 62.5).abs() < 1e-6);
    }
}
