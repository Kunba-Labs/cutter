//! Burned captions as an ASS file for libass: the current word highlighted,
//! an optional translated line under it, the hook in the first seconds and
//! a small watermark. Times are relative to the cut (clip start = 0).

use crate::model::{CaptionStyle, Segment};

pub struct CaptionSpec<'a> {
    pub segments: &'a [Segment],
    pub translation: &'a [Segment],
    pub clip_start: f64,
    pub clip_end: f64,
    pub width: i64,
    pub height: i64,
    /// Extra bottom margin as a fraction (TikTok's UI covers more).
    pub extra_bottom: f64,
    pub style: &'a CaptionStyle,
    pub language: &'a str,
    pub hook: &'a str,
    pub watermark: &'a str,
    /// Show the translated lines instead of the spoken words (one language, never both).
    pub use_translation: bool,
}

fn ass_color(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return "&H00FFFFFF".into();
    }
    format!("&H00{}{}{}", &h[4..6], &h[2..4], &h[0..2])
}

fn ts(t: f64) -> String {
    let t = t.max(0.0);
    let cs = ((t - t.floor()) * 100.0).round() as i64;
    let s = t.floor() as i64;
    format!("{}:{:02}:{:02}.{:02}", s / 3600, (s % 3600) / 60, s % 60, cs.min(99))
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('{', "(").replace('}', ")").replace('\n', " ")
}

pub fn is_rtl(lang: &str) -> bool {
    matches!(lang, "ar" | "ur" | "fa" | "he" | "ps" | "sd")
}

/// Only the styled title, shown for a minute: burned onto the cover frame.
pub fn cover(style: &CaptionStyle, width: i64, height: i64, extra_bottom: f64, language: &str, title: &str, watermark: &str) -> String {
    let mut st = style.clone();
    st.hook = true;
    st.hook_seconds = 60.0;
    // A thumbnail is read at a fraction of the size, so the title grows by a quarter.
    st.hook_size = (st.hook_size.max(12) as f64 * 1.25) as i64;
    build(&CaptionSpec { segments: &[], translation: &[], clip_start: 0.0, clip_end: 60.0, width, height, extra_bottom, style: &st, language, hook: title, watermark, use_translation: false })
}

pub fn build(c: &CaptionSpec) -> String {
    let st = c.style;
    let rtl = is_rtl(c.language);
    let font = if rtl { "Geeza Pro" } else { st.font.as_str() };
    let size = if rtl { (st.size as f64 * 1.15) as i64 } else { st.size };
    let bottom = (c.height as f64 * (st.position_pct as f64 / 100.0 + c.extra_bottom)) as i64;
    let white = "&H00FFFFFF";
    let hl = ass_color(&st.highlight);
    let text = ass_color(&st.text_color);
    let primary = text.as_str();
    let (outline_w, shadow, border_style, back) = if st.boxed { (0, 0, 3, "&H99000000") } else { (st.outline_px.max(0), if st.outline_px > 0 { 1 } else { 0 }, 1, "&H80000000") };
    let align = if st.align == "left" || st.preset == "lower" { 1 } else { 2 };
    let margin_l = if align == 1 { 60 } else { 40 };
    let bold = if st.bold { -1 } else { 0 };
    let mut out = format!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: {w}\nPlayResY: {h}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Main,{font},{size},{primary},{white},&H00000000,{back},{bold},0,0,0,100,100,0,0,{border_style},{outline_w},{shadow},{align},{margin_l},40,{bottom},1\n\
Style: Trans,{font},{tsize},&H00F0F0F0,{white},&H00000000,&H99000000,0,0,0,0,100,100,0,0,3,0,0,2,60,60,{tbottom},1\n\
Style: Hook,{hfont},{hsize},{hcolor},{white},&H00000000,{hback},{hbold},{hitalic},0,0,100,100,0,0,{hborder},{houtline},0,8,60,60,{htop},1\n\
Style: Mark,{font},{msize},&H00FFFFFF,{white},&H00000000,&H66000000,-1,0,0,0,100,100,0,0,3,0,0,1,40,40,60,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        w = c.width,
        h = c.height,
        tsize = (size as f64 * 0.55) as i64,
        tbottom = bottom - (size as f64 * 1.9) as i64,
        hfont = if st.hook_font.is_empty() { font } else { st.hook_font.as_str() },
        hsize = st.hook_size.max(12),
        hcolor = ass_color(&st.hook_color),
        hback = if st.hook_boxed { format!("&H33{}", &ass_color(&st.hook_box_color)[4..]) } else { "&H80000000".to_string() },
        hbold = if st.hook_bold { -1 } else { 0 },
        hitalic = if st.hook_italic { -1 } else { 0 },
        hborder = if st.hook_boxed { 3 } else { 1 },
        houtline = if st.hook_boxed { 0 } else { 3 },
        htop = (c.height as f64 * st.hook_pct as f64 / 100.0) as i64,
        msize = (size as f64 * 0.45) as i64,
    );
    let dur = c.clip_end - c.clip_start;
    let hl_tag = format!("{{\\c{hl}&}}");
    let base_tag = format!("{{\\c{primary}&}}");
    let rtl_tag = if rtl { "{\\q2}" } else { "" };

    let translated: Vec<&Segment> = c.translation.iter().filter(|t| t.end > c.clip_start && t.start < c.clip_end && !t.text.trim().is_empty()).collect();
    if c.use_translation && !translated.is_empty() {
        // Translated captions: one line per transcript segment, no word timing to highlight.
        for t in translated {
            let line = if st.uppercase { t.text.to_uppercase() } else { t.text.clone() };
            out.push_str(&format!("Dialogue: 0,{},{},Main,,0,0,0,,{}\n", ts(t.start.max(c.clip_start) - c.clip_start), ts(t.end.min(c.clip_end) - c.clip_start), esc(&line)));
        }
    } else {
        // Spoken words inside the cut, grouped into lines of N words, the current word highlighted.
        let mut words: Vec<(String, f64, f64)> = Vec::new();
        for s in c.segments.iter().filter(|s| s.end > c.clip_start && s.start < c.clip_end) {
            if s.words.is_empty() {
                let ws: Vec<&str> = s.text.split_whitespace().collect();
                let n = ws.len().max(1) as f64;
                for (i, w) in ws.iter().enumerate() {
                    let a = s.start + (s.end - s.start) * i as f64 / n;
                    words.push((w.to_string(), a, s.start + (s.end - s.start) * (i as f64 + 1.0) / n));
                }
            } else {
                for w in &s.words {
                    words.push((w.w.clone(), w.s, w.e));
                }
            }
        }
        words.retain(|(_, s, e)| *e > c.clip_start && *s < c.clip_end);
        let per = st.words_per_line.max(1) as usize;
        let mut i = 0;
        while i < words.len() {
            let line = &words[i..(i + per).min(words.len())];
            for (k, (_, ws, _)) in line.iter().enumerate() {
                let start = (*ws).max(c.clip_start) - c.clip_start;
                let next = if k + 1 < line.len() { line[k + 1].1 } else if i + per < words.len() { words[i + per].1 } else { line[k].2 + 0.4 };
                let end = (next.min(c.clip_end) - c.clip_start).max(start + 0.05);
                let text: Vec<String> = line
                    .iter()
                    .enumerate()
                    .map(|(j, (w, _, _))| {
                        let w = if st.uppercase { w.to_uppercase() } else { w.clone() };
                        if j == k && !st.highlight.eq_ignore_ascii_case(&st.text_color) { format!("{hl_tag}{}{base_tag}", esc(&w)) } else { esc(&w) }
                    })
                    .collect();
                out.push_str(&format!("Dialogue: 0,{},{},Main,,0,0,0,,{rtl_tag}{}\n", ts(start), ts(end), text.join(" ")));
            }
            i += per;
        }
    }
    if st.hook && !c.hook.trim().is_empty() {
        let text = if st.hook_uppercase { c.hook.to_uppercase() } else { c.hook.to_string() };
        out.push_str(&format!("Dialogue: 2,{},{},Hook,,0,0,0,,{rtl_tag}{}\n", ts(0.0), ts(st.hook_seconds.max(0.5).min(dur)), esc(&text)));
    }
    if st.watermark && !c.watermark.trim().is_empty() {
        out.push_str(&format!("Dialogue: 2,{},{},Mark,,0,0,0,,{}\n", ts(0.0), ts(dur), esc(c.watermark)));
    }
    out
}

/// Plain SRT of the cut, for the sidecar and for platforms that take captions.
pub fn srt(segments: &[Segment], clip_start: f64, clip_end: f64) -> String {
    let fmt = |t: f64| {
        let t = t.max(0.0);
        let ms = ((t - t.floor()) * 1000.0).round() as i64;
        let s = t.floor() as i64;
        format!("{:02}:{:02}:{:02},{:03}", s / 3600, (s % 3600) / 60, s % 60, ms.min(999))
    };
    segments
        .iter()
        .filter(|s| s.end > clip_start && s.start < clip_end)
        .enumerate()
        .map(|(i, s)| format!("{}\n{} --> {}\n{}\n", i + 1, fmt(s.start.max(clip_start) - clip_start), fmt(s.end.min(clip_end) - clip_start), s.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Word;
    #[test]
    fn karaoke_lines() {
        let segs = vec![Segment { start: 10.0, end: 12.0, text: "sabr is niet wachten".into(), words: vec![Word { w: "sabr".into(), s: 10.0, e: 10.4 }, Word { w: "is".into(), s: 10.5, e: 10.7 }, Word { w: "niet".into(), s: 10.8, e: 11.1 }, Word { w: "wachten".into(), s: 11.2, e: 12.0 }] }];
        let st = CaptionStyle::default();
        let ass = build(&CaptionSpec { segments: &segs, translation: &[], clip_start: 10.0, clip_end: 12.0, width: 1080, height: 1920, extra_bottom: 0.0, style: &st, language: "nl", hook: "Sabr ≠ wachten", watermark: "Al-Qadri", use_translation: false });
        assert_eq!(ass.matches("Style: Main").count(), 1);
        assert_eq!(ass.matches("Dialogue: 0,").count(), 4);
        assert!(ass.contains("&H00A3F23E")); // #3EF2A3 as BGR
        assert!(ass.contains("Hook,,0,0,0,,Sabr"));
        assert_eq!(ts(61.5), "0:01:01.50");
        assert!(srt(&segs, 10.0, 12.0).starts_with("1\n00:00:00,000 --> 00:00:02,000\nsabr"));
    }
}
