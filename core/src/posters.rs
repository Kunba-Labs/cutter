//! Posters for lessons and events: Claude Code (with the Higgsfield MCP the
//! person already has) makes the variants, the QR goes in locally, ffmpeg makes
//! the feed/story crops and the "starting soon" loop.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use crate::model::{Poster, Settings};
use crate::Library;

pub const ADD_QR: &str = include_str!("../../tools/add_qr.py");

pub fn qr_script(data_dir: &Path) -> PathBuf {
    let p = data_dir.join("add_qr.py");
    if std::fs::read_to_string(&p).map(|s| s != ADD_QR).unwrap_or(true) {
        let _ = std::fs::write(&p, ADD_QR);
    }
    p
}

fn field<'a>(p: &'a Poster, k: &str) -> &'a str {
    p.fields[k].as_str().unwrap_or("")
}

/// The brief Claude gets. It knows the recipe (three fresh palettes, the face
/// reference, the blank white panel for the QR) and where to put the files.
pub fn generation_prompt(p: &Poster, s: &Settings, dir: &Path) -> String {
    let lines: Vec<String> = ["title", "subtitle", "date", "speaker", "programme", "location", "note"].iter().filter(|k| !field(p, k).is_empty()).map(|k| format!("- {k}: {}", field(p, k))).collect();
    let history = if s.poster_history.is_empty() { "none yet".to_string() } else { s.poster_history.join("; ") };
    let reference = if s.poster_reference_media.is_empty() { "No reference photo: no portrait on the poster.".to_string() } else { format!("Reference photo of the speaker = Higgsfield media id {} (role image). Keep his face and features accurate: beard, glasses, embroidered cap and cream robe.", s.poster_reference_media) };
    let extra = field(p, "brief");
    format!(
        r#"Make 3 poster variants for this event with the Higgsfield MCP (generate_image_batch, model gpt_image_2_5, aspect_ratio "2:3", quality "high"), each in a different palette and style. Palettes and styles used before, do NOT repeat: {history}.

{reference}

Text on the poster, exactly these lines, in this order:
{lines}

Every variant must have a flat pure-white EMPTY square panel near the bottom (no code, no dots, no text inside) with the caption "{qr_caption}" under it. The QR code is pasted in afterwards by a script.
{extra}

When the three images are done, download each result_url with curl into this folder, named variant-1.png, variant-2.png and variant-3.png:
{dir}

Then print one line per variant: "PALETTE <n>: <palette and style in a few words>" and finish with the word DONE. Do nothing else."#,
        lines = lines.join("\n"),
        qr_caption = field(p, "qrCaption").to_string().replace("\"", "'"),
        dir = dir.display(),
    )
}

pub fn generate(lib: &Library, p: &Poster, on_spawn: impl FnOnce(u32)) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    let s = lib.settings();
    let dir = PathBuf::from(&p.folder);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let prompt = generation_prompt(p, &s, &dir);
    let text = crate::brain::ask(&Settings { brain: "claude".into(), ..s.clone() }, &prompt, Duration::from_secs(20 * 60), on_spawn)?;
    let mut files: Vec<PathBuf> = (1..=3).map(|i| dir.join(format!("variant-{i}.png"))).filter(|f| f.exists()).collect();
    if files.is_empty() {
        // Claude sometimes names them differently; take any fresh png in the folder.
        files = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "png").unwrap_or(false) && !p.file_name().unwrap().to_string_lossy().contains("-qr")).collect();
    }
    if files.is_empty() {
        return Err(format!("Claude produced no images. Its answer: {}", text.chars().take(600).collect::<String>()));
    }
    let palettes: Vec<String> = text.lines().filter_map(|l| l.trim().strip_prefix("PALETTE ")).map(|l| l.splitn(2, ':').nth(1).unwrap_or(l).trim().to_string()).collect();
    Ok((files, palettes))
}

/// Paste and verify the QR; returns the new file, or the reason it failed.
pub fn add_qr(data_dir: &Path, input: &Path, link: &str) -> Result<PathBuf, String> {
    let script = qr_script(data_dir);
    let out = input.with_file_name(format!("{}-qr.png", input.file_stem().unwrap().to_string_lossy()));
    crate::tools::run(Command::new("uv").args(["run", "--with", "qrcode[pil]", "--with", "opencv-python-headless", "python"]).arg(&script).arg(input).arg(&out).arg(link))?;
    Ok(out)
}

/// print png · share jpeg 1600 · feed 4:5 · story 9:16
pub fn export(p: &Poster, s: &Settings) -> Result<Value, String> {
    let chosen = PathBuf::from(p.chosen.as_deref().ok_or("choose a variant first")?);
    let dir = PathBuf::from(&p.folder);
    let stem = crate::model::slug(&format!("{} {}", p.title, field(p, "date")));
    let print = dir.join(format!("{stem}-print.png"));
    std::fs::copy(&chosen, &print).map_err(|e| e.to_string())?;
    let jpeg = dir.join(format!("{stem}-1600.jpg"));
    crate::tools::run(Command::new("sips").args(["-s", "format", "jpeg", "-s", "formatOptions", "85", "-Z", "1600"]).arg(&print).arg("--out").arg(&jpeg))?;
    let feed = dir.join(format!("{stem}-feed-4x5.jpg"));
    crate::ffmpeg::poster_pad(&print, &feed, 1080, 1350)?;
    let story = dir.join(format!("{stem}-story-9x16.jpg"));
    crate::ffmpeg::poster_pad(&print, &story, 1080, 1920)?;
    let _ = s;
    Ok(json!({ "print": print, "jpeg": jpeg, "feed": feed, "story": story }))
}

pub struct WaitingArgs {
    pub start_at: Option<chrono::DateTime<chrono::Utc>>,
    pub lines: Vec<String>,
    pub loop_s: i64,
    pub countdown: bool,
}

/// waiting-16x9.mp4 · waiting-9x16.mp4 · break and ended stills, from the chosen poster.
pub fn waiting_video(p: &Poster, a: &WaitingArgs, font: &str, mut on_progress: impl FnMut(f64, &str)) -> Result<Value, String> {
    let chosen = PathBuf::from(p.chosen.as_deref().ok_or("choose a variant first")?);
    let dir = PathBuf::from(&p.folder);
    let countdown_from = if a.countdown { a.start_at.map(|t| (t - chrono::Utc::now()).num_seconds().max(0)) } else { None };
    let mut out = json!({});
    for (name, w, h) in [("waiting-16x9.mp4", 1920, 1080), ("waiting-9x16.mp4", 1080, 1920)] {
        on_progress(if w > h { 0.05 } else { 0.5 }, name);
        let path = dir.join(name);
        crate::ffmpeg::waiting_video(&crate::ffmpeg::WaitingSpec { poster: &chosen, out: &path, width: w, height: h, lines: &a.lines, countdown_from, loop_s: a.loop_s, font })?;
        out[name.trim_end_matches(".mp4")] = json!(path);
    }
    for (name, text) in [("break-16x9.png", "We zijn zo terug"), ("ended-16x9.png", "Tot volgende week, in sha Allah")] {
        let path = dir.join(name);
        crate::ffmpeg::still(&chosen, &path, 1920, 1080, text, font)?;
        out[name.trim_end_matches(".png")] = json!(path);
    }
    on_progress(1.0, "done");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_mentions_folder_and_lines() {
        let p = Poster { title: "Tafsir".into(), fields: json!({ "date": "vrijdag 2 oktober", "qrCaption": "Scan" }), ..Default::default() };
        let s = Settings { poster_history: vec!["emerald + gold".into()], ..Default::default() };
        let t = generation_prompt(&p, &s, Path::new("/tmp/x"));
        assert!(t.contains("- date: vrijdag 2 oktober") && t.contains("emerald + gold") && t.contains("/tmp/x") && t.contains("Scan"));
        assert!(ADD_QR.contains("sys.argv[3]"));
    }
}

/// End cards for the reels, drawn in this poster's style. Returns aspect key → file.
pub fn generate_end_cards(lib: &Library, p: &Poster, on_spawn: impl FnOnce(u32)) -> Result<std::collections::HashMap<&'static str, PathBuf>, String> {
    let s = lib.settings();
    let dir = PathBuf::from(&p.folder);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let reference = p.chosen.as_deref().map(|c| format!("Match the palette, typography and ornament of the poster at {c} (upload it with media_upload and pass it as a style reference).")).unwrap_or_default();
    let channel = if s.channel_name.is_empty() { "the channel".to_string() } else { s.channel_name.clone() };
    let brief = field(p, "endCardBrief");
    let prompt = format!(
        r##"Make 3 closing cards for short videos with the Higgsfield MCP (generate_image_batch, model gpt_image_2_5, quality "high"): one with aspect_ratio "9:16", one "3:2" landscape (we crop it to 16:9), and one "2:3" portrait (we crop it to 4:5). Keep every important element well inside the middle 80% so cropping is safe.

{reference}

Content of each card, large and readable on a phone: the name "{channel}", the line "Volledige les op YouTube", and a small "abonneer" hint. No QR code, no date, no programme lines. Flat, calm, no photos of people.
{brief}

Download the results with curl into this folder as endcard-9x16-src.png, endcard-16x9-src.png and endcard-4x5-src.png:
{dir}

Finish with the word DONE."##,
        dir = dir.display()
    );
    let _ = crate::brain::ask(&Settings { brain: "claude".into(), ..s.clone() }, &prompt, Duration::from_secs(20 * 60), on_spawn)?;
    let mut out = std::collections::HashMap::new();
    for key in ["9x16", "16x9", "4x5"] {
        let f = dir.join(format!("endcard-{key}-src.png"));
        if f.exists() {
            out.insert(match key { "9x16" => "9x16", "16x9" => "16x9", _ => "4x5" }, f);
        }
    }
    Ok(out)
}
