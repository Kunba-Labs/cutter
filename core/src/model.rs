//! Records as the UI, the CLI and the MCP see them. camelCase on the wire.
//! ponytail: list/object-valued columns are JSON text in SQLite.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    /// "youtube" | "file" | "channel" (found by a watched channel)
    pub kind: String,
    pub url: Option<String>,
    pub path: Option<String>,
    pub title: String,
    pub channel: Option<String>,
    pub channel_id: Option<String>,
    pub duration: Option<f64>,
    /// Detected language (ISO 639-1), and the person's override if any.
    pub language: Option<String>,
    pub lang_override: Option<String>,
    /// added → downloading → downloaded → transcribing → transcribed → detecting → review → done | failed
    pub stage: String,
    pub error: Option<String>,
    pub folder: String,
    pub video_path: Option<String>,
    pub thumb_path: Option<String>,
    /// A 1080p H.264 copy for the in-app player when the original is VP9/AV1/4K. Renders use the original.
    #[serde(default)]
    pub preview_path: Option<String>,
    pub captions_path: Option<String>,
    /// "captions" | "whisper" | "both"
    pub transcript_source: String,
    pub auto_detect: bool,
    pub auto_approve_score: i64,
    pub auto_schedule: bool,
    pub meta: Value,
    pub added_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub url: String,
    pub name: String,
    pub interval_h: i64,
    pub min_len_s: i64,
    pub title_regex: String,
    pub since: String,
    pub auto_process: bool,
    pub auto_approve_score: i64,
    pub auto_schedule: bool,
    pub enabled: bool,
    pub last_check: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub id: String,
    pub channel_id: String,
    pub video_id: String,
    pub url: String,
    pub title: String,
    pub duration: Option<f64>,
    pub uploaded_at: Option<String>,
    /// "new" | "processed" | "skipped" | "below_min" | "filtered"
    pub status: String,
    pub source_id: Option<String>,
    pub found_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Word {
    pub w: String,
    pub s: f64,
    pub e: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub source_id: String,
    pub engine: String,
    pub language: String,
    pub segments: Vec<Segment>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub source_id: String,
    pub start: f64,
    pub end: f64,
    pub category: String,
    pub score: i64,
    pub title: String,
    pub hook: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub why: String,
    /// Translated lines aligned to transcript segments inside the reel.
    pub translation: Vec<Segment>,
    pub approved: bool,
    pub discarded: bool,
    /// {"x": 0.5} — horizontal centre of the vertical crop as a fraction.
    pub crop: Value,
    pub style: Option<String>,
    /// Template whose title look is used, when different from `style`.
    pub hook_style: Option<String>,
    /// Caption distance from the bottom (% of height) for this reel, else the template's.
    pub caption_pct: Option<i64>,
    /// false: no captions burned in (None = on).
    #[serde(default)]
    pub captions_on: Option<bool>,
    /// false: no title on the video (None = on).
    #[serde(default)]
    pub title_on: Option<bool>,
    /// Source time of the frame the brain picked as the cover. Set on the first render, reused after.
    #[serde(default)]
    pub cover_t: Option<f64>,
    /// The longer line on the cover image that tells the reel's story. Written by the brain with the
    /// frame pick when empty; the on-video title (`hook`) stands in when it is.
    #[serde(default)]
    pub cover_title: String,
    /// Target ids this reel goes to. Empty: every enabled target.
    #[serde(default)]
    pub targets: Vec<String>,
    /// Override of settings.formats, or empty for the default.
    pub formats: Vec<String>,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Render {
    pub id: String,
    pub candidate_id: String,
    pub source_id: String,
    pub format: String,
    pub path: String,
    pub cover_path: Option<String>,
    pub srt_path: Option<String>,
    pub status: String,
    pub error: Option<String>,
    /// Measured after the render: width, height, fps, vcodec, kbps, seconds, mb, issues (empty = high quality).
    #[serde(default)]
    pub info: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: String,
    pub render_id: Option<String>,
    pub candidate_id: Option<String>,
    pub poster_id: Option<String>,
    /// The target's kind: "youtube" | "tiktok" | "instagram" | "facebook" | "folder"
    pub channel: String,
    /// The Target this post goes to.
    #[serde(default)]
    pub target_id: String,
    pub scheduled_at: String,
    /// planned (auto-filled) | confirmed | posting | posted | failed
    pub status: String,
    pub title: String,
    pub caption: String,
    pub provider_id: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Somewhere a reel or poster goes: a YouTube channel with its own login, a manual
/// Instagram/TikTok/Facebook account (file + caption prepared), or a drop folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Target {
    pub id: String,
    /// "youtube" | "instagram" | "tiktok" | "facebook" | "folder"
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    /// Local hours a post goes out, e.g. [17]. Empty: never auto-filled.
    pub hours: Vec<i64>,
    /// Render format this target takes; empty = the kind's default.
    pub format: String,
    /// Planned posts go out without a confirm.
    pub auto_schedule: bool,
    // youtube
    pub refresh_token: String,
    pub channel_title: String,
    pub privacy: String,
    pub category_id: String,
    pub title_suffix: String,
    pub description_footer: String,
    // folder
    pub path: String,
    pub created_at: String,
}

impl Default for Target {
    fn default() -> Self {
        Self { id: String::new(), kind: "youtube".into(), name: String::new(), enabled: true, hours: vec![], format: String::new(), auto_schedule: false, refresh_token: String::new(), channel_title: String::new(), privacy: "public".into(), category_id: "27".into(), title_suffix: " #Shorts".into(), description_footer: "Volledige les: {source_url}".into(), path: String::new(), created_at: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Poster {
    pub id: String,
    pub template: String,
    pub title: String,
    /// Free-form: date, speaker, location, programme lines, qr link, reference photo…
    pub fields: Value,
    pub folder: String,
    pub variants: Vec<String>,
    pub chosen: Option<String>,
    pub qr_ok: bool,
    /// Exported files by name: print, jpeg, feed, story, waiting16x9, waiting9x16…
    pub outputs: Value,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for Poster {
    fn default() -> Self {
        Self { id: String::new(), template: String::new(), title: String::new(), fields: serde_json::json!({}), folder: String::new(), variants: Vec::new(), chosen: None, qr_ok: false, outputs: serde_json::json!({}), status: String::new(), error: None, created_at: String::new(), updated_at: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    /// download | transcribe | detect | render | publish | poster | waiting_video | check_channel
    pub kind: String,
    pub ref_id: String,
    pub label: String,
    /// queued | running | done | failed | cancelled
    pub status: String,
    pub progress: f64,
    pub message: String,
    pub log: String,
    pub args: Value,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptionStyle {
    /// Template name; a reel's `style` refers to one of these.
    pub name: String,
    /// karaoke (current word highlighted) | lower (left-aligned bar). Kept for older libraries.
    pub preset: String,
    pub font: String,
    pub size: i64,
    /// Distance from the bottom as a percentage of the frame height.
    pub position_pct: i64,
    pub words_per_line: i64,
    pub text_color: String,
    pub highlight: String,
    pub outline_px: i64,
    pub boxed: bool,
    pub uppercase: bool,
    pub bold: bool,
    /// center | left
    pub align: String,
    pub hook: bool,
    /// The title shown at the top for the first seconds: its own look.
    pub hook_font: String,
    pub hook_size: i64,
    pub hook_color: String,
    pub hook_boxed: bool,
    pub hook_box_color: String,
    pub hook_uppercase: bool,
    pub hook_bold: bool,
    pub hook_italic: bool,
    /// Distance from the top as a percentage of the frame height.
    pub hook_pct: i64,
    pub hook_seconds: f64,
    pub watermark: bool,
    pub translation: bool,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            name: "Karaoke".into(),
            preset: "karaoke".into(),
            font: "Helvetica Neue".into(),
            size: 42,
            position_pct: 26,
            words_per_line: 4,
            text_color: "#FFFFFF".into(),
            highlight: "#3EF2A3".into(),
            outline_px: 3,
            boxed: false,
            uppercase: false,
            bold: true,
            align: "center".into(),
            hook: true,
            hook_font: String::new(),
            hook_size: 40,
            hook_color: "#FFFFFF".into(),
            hook_boxed: true,
            hook_box_color: "#000000".into(),
            hook_uppercase: false,
            hook_bold: true,
            hook_italic: false,
            hook_pct: 8,
            hook_seconds: 2.5,
            watermark: true,
            translation: false,
        }
    }
}

/// The looks that work on Shorts, Reels and TikTok. Editable copies live in settings.
pub fn caption_templates() -> Vec<CaptionStyle> {
    let base = CaptionStyle::default();
    vec![
        base.clone(),
        CaptionStyle { name: "Bold caps".into(), size: 54, position_pct: 34, words_per_line: 3, highlight: "#FFD400".into(), outline_px: 6, uppercase: true, hook_size: 56, hook_color: "#FFD400".into(), hook_boxed: false, hook_uppercase: true, hook_pct: 10, ..base.clone() },
        CaptionStyle { name: "Clean".into(), size: 40, words_per_line: 5, highlight: "#FFFFFF".into(), outline_px: 2, bold: false, hook_font: "Georgia".into(), hook_size: 46, hook_boxed: false, hook_bold: false, hook_italic: true, ..base.clone() },
        CaptionStyle { name: "Boxed".into(), size: 40, words_per_line: 4, highlight: "#3EF2A3".into(), outline_px: 0, boxed: true, hook_size: 38, hook_boxed: true, hook_box_color: "#3EF2A3".into(), hook_color: "#14061A".into(), ..base.clone() },
        CaptionStyle { name: "Magenta".into(), size: 46, highlight: "#F52ACB".into(), outline_px: 5, hook_size: 44, hook_boxed: true, hook_box_color: "#F52ACB".into(), hook_color: "#FFFFFF".into(), hook_uppercase: true, ..base.clone() },
        CaptionStyle { name: "Lower third".into(), preset: "lower".into(), size: 36, position_pct: 12, words_per_line: 6, highlight: "#FFFFFF".into(), outline_px: 0, boxed: true, align: "left".into(), hook_size: 30, hook_boxed: true, hook_uppercase: true, hook_pct: 6, ..base.clone() },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct YoutubeAuth {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub channel_title: String,
    pub privacy: String,
    pub category_id: String,
    pub title_suffix: String,
    pub description_footer: String,
}

/// Cuttar's own OAuth client (Google Cloud project "cuttar", Desktop app, internal to sadiq.nl).
/// A desktop app is a public client, so the secret ships in the binary like OBS does; Connect is then one click.
pub const YOUTUBE_CLIENT_ID: &str = "823828128291-aih0lbtrpqvikff1nurpnr51su9fgi6g.apps.googleusercontent.com";
pub const YOUTUBE_CLIENT_SECRET: &str = "***REMOVED***";

impl Default for YoutubeAuth {
    fn default() -> Self {
        Self {
            client_id: YOUTUBE_CLIENT_ID.into(),
            client_secret: YOUTUBE_CLIENT_SECRET.into(),
            refresh_token: String::new(),
            channel_title: String::new(),
            privacy: "public".into(),
            category_id: "27".into(),
            title_suffix: " #Shorts".into(),
            description_footer: "Volledige les: {source_url}".into(),
        }
    }
}

/// The closing card appended to every reel: one image per aspect.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EndCard {
    pub enabled: bool,
    pub seconds: f64,
    /// "9x16" | "4x5" | "16x9" → image path
    pub paths: Value,
    /// The poster the set came from, for the UI.
    pub poster_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub out_dir: String,
    pub whisper_model: String,
    pub whisper_fallback: String,
    /// "claude" | "ollama"
    pub brain: String,
    pub claude_bin: String,
    pub ollama_model: String,
    pub max_reel_s: i64,
    pub min_score: i64,
    pub auto_detect: bool,
    /// Claude fixes clear transcript errors (word for word) before the reel search.
    pub polish_captions: bool,
    /// Workers take no new jobs while set; running ones finish.
    pub queue_paused: bool,
    pub auto_approve_score: i64,
    pub translate_to: String,
    pub caption_style: CaptionStyle,
    pub caption_templates: Vec<CaptionStyle>,
    pub end_card: EndCard,
    /// "best" | "good" | "fast", see ffmpeg::encoder_args.
    pub render_quality: String,
    pub formats: Vec<String>,
    pub channel_name: String,
    pub glossary: Vec<String>,
    pub youtube: YoutubeAuth,
    pub parallel_jobs: i64,
    /// Per channel: at which local hours a reel goes out, e.g. {"youtube": [17], "instagram": [18]}
    pub cadence: Value,
    pub poster_reference_media: String,
    pub poster_history: Vec<String>,
    pub whatsapp_link: String,
}

impl Default for Settings {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        Self {
            out_dir: format!("{home}/Movies/Cuttar"),
            whisper_model: "mlx-community/whisper-large-v3-turbo".into(),
            whisper_fallback: "mlx-community/whisper-medium".into(),
            brain: "claude".into(),
            claude_bin: "claude".into(),
            ollama_model: "gemma4:26b-mlx".into(),
            max_reel_s: 40,
            min_score: 6,
            auto_detect: true,
            polish_captions: true,
            queue_paused: false,
            auto_approve_score: 0,
            translate_to: String::new(),
            caption_style: CaptionStyle::default(),
            caption_templates: caption_templates(),
            end_card: EndCard { enabled: false, seconds: 2.5, paths: serde_json::json!({}), poster_id: None },
            render_quality: "best".into(),
            formats: vec!["shorts".into(), "reels".into(), "tiktok".into()],
            channel_name: String::new(),
            glossary: vec!["Allah".into(), "salawat".into(), "dhikr".into(), "tafsir".into(), "sabr".into()],
            youtube: YoutubeAuth::default(),
            parallel_jobs: 2,
            cadence: serde_json::json!({ "youtube": [17], "instagram": [18], "tiktok": [19] }),
            poster_reference_media: String::new(),
            poster_history: Vec::new(),
            whatsapp_link: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    pub ffmpeg: Option<String>,
    /// ffmpeg was built with libass (burned captions need it).
    pub ffmpeg_ass: bool,
    pub ytdlp: Option<String>,
    pub whisper: Option<String>,
    pub claude: Option<String>,
    pub ollama: Option<String>,
    pub uv: Option<String>,
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn new_id(prefix: &str) -> String {
    use rand::RngCore;
    let mut b = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut b);
    format!("{prefix}-{}", hex(&b))
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// "tafsir-juz-amma-surah-al-balad" from a title; folder names and file stems.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "untitled".into() } else { out }
}

pub fn fmt_time(s: f64) -> String {
    let t = s.max(0.0);
    let (h, m, sec) = ((t / 3600.0) as i64, ((t % 3600.0) / 60.0) as i64, t % 60.0);
    if h > 0 { format!("{h}:{m:02}:{sec:04.1}") } else { format!("{m}:{sec:04.1}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slugs_and_times() {
        assert_eq!(slug("Tafsir Juz 'Amma — Surah al-Balad (les 12)"), "tafsir-juz-amma-surah-al-balad-les-12");
        assert_eq!(slug("!!!"), "untitled");
        assert_eq!(fmt_time(721.25), "12:01.2");
        assert_eq!(fmt_time(3725.0), "1:02:05.0");
    }
}
