//! The reel finder: the timed transcript goes to `claude -p` (or ollama) and a
//! JSON list of complete, ≤ max-second moments comes back. Segment indices in
//! and out, never raw timestamps — the model can't mistype a number it never sees.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::model::{fmt_time, Candidate, Segment, Settings};

pub struct Draft {
    pub start_seg: usize,
    pub end_seg: usize,
    pub category: String,
    pub score: i64,
    pub title: String,
    pub hook: String,
    pub caption: String,
    pub hashtags: Vec<String>,
    pub why: String,
    pub translation: Vec<String>,
}

pub const CATEGORIES: &[&str] = &["fact", "statement", "hook", "story", "dua", "reminder", "qa"];

pub fn prompt(segments: &[Segment], offset: usize, title: &str, language: &str, s: &Settings) -> String {
    let lines: Vec<String> = segments.iter().enumerate().map(|(i, g)| format!("#{} [{}–{}] {}", i + offset, fmt_time(g.start), fmt_time(g.end), g.text.trim())).collect();
    let translate = if s.translate_to.is_empty() || s.translate_to == language {
        String::new()
    } else {
        format!("\n- \"translation\": one {} translation per transcript line from start_seg to end_seg, in order (array of strings, same length as the span). Keep it as short as the original.", lang_name(&s.translate_to))
    };
    format!(
        r##"You pick short-form reels out of a transcript of an Islamic lecture. Title: "{title}". Spoken language: {lang}.

The transcript is numbered lines: "#index [start–end] text". Find the moments that stand alone as a reel: a complete thought that needs no context before it and does not stop mid-sentence. Prefer facts, clear statements, catchy openers, short stories, du'as, reminders, and good question-and-answer exchanges. Skip housekeeping, greetings, announcements, and anything that only makes sense with the lesson around it.

Hard rules:
- A reel is at most {max} seconds long, measured from the start of its first line to the end of its last line. Longer is not allowed; pick a shorter span instead.
- A reel is at least 8 seconds.
- Start on the first line of a sentence and end on the line where that sentence or thought closes.
- Reels must not overlap.
- Return between 4 and 12 reels, the best ones, scored honestly.

Return ONLY a JSON array, no prose, each item:
- "start_seg": index of the first line
- "end_seg": index of the last line (inclusive)
- "category": one of fact, statement, hook, story, dua, reminder, qa
- "score": 1–10, how strong this is as a standalone reel
- "title": short title in the spoken language (max 60 chars)
- "hook": 2–5 words shown on screen in the first seconds, in the spoken language
- "caption": 1–2 sentence caption for the post in the spoken language, ending with the source ("— {title}")
- "hashtags": 4–6 hashtags without the # sign
- "why": one line, in English, on why this works alone{translate}

Transcript:
{lines}"##,
        lang = lang_name(language),
        max = s.max_reel_s,
        lines = lines.join("\n"),
    )
}

pub fn lang_name(code: &str) -> &str {
    match code {
        "nl" => "Dutch",
        "en" => "English",
        "ur" => "Urdu",
        "ar" => "Arabic",
        "tr" => "Turkish",
        "fr" => "French",
        "de" => "German",
        "id" => "Indonesian",
        "ms" => "Malay",
        "bn" => "Bengali",
        "hi" => "Hindi",
        "fa" => "Persian",
        "so" => "Somali",
        "" => "unknown",
        other => other,
    }
}

/// Ask the configured brain; returns the model's raw text.
/// Asks for the best cover frame on a numbered 3×3 contact sheet. Both brains read a file path in the prompt.
pub fn cover_prompt(sheet: &std::path::Path, title: &str) -> String {
    format!(
        "Look at the image file {} (read it with your Read tool). It is a 3 by 3 contact sheet of nine numbered frames \
from a short vertical clip of an Islamic lecture titled \"{title}\". Pick the one frame that works best as the cover \
image of the clip on Instagram, TikTok and YouTube Shorts: the speaker's face sharp and clearly visible, eyes open, \
no mid-blink or mid-word grimace, a natural or expressive look, well framed, no motion blur, no slide or transition. \
Answer with JSON only: {{\"frame\": <1-9>, \"why\": \"<ten words>\"}}",
        sheet.display()
    )
}

/// The 1-based frame number in the brain's answer.
pub fn parse_frame(text: &str) -> Option<usize> {
    let (start, end) = (text.find('{')?, text.rfind('}')?);
    let v: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let n = v["frame"].as_u64().or_else(|| v["frame"].as_str().and_then(|s| s.trim().parse().ok()))? as usize;
    (1..=9).contains(&n).then_some(n)
}

pub fn ask(s: &Settings, prompt: &str, timeout: Duration, on_spawn: impl FnOnce(u32)) -> Result<String, String> {
    let child = if s.brain == "ollama" {
        let mut c = Command::new("ollama").args(["run", &s.ollama_model]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("ollama: {e}"))?;
        let mut stdin = c.stdin.take().unwrap();
        let p = prompt.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(p.as_bytes());
        });
        c
    } else {
        // Claude Code, non-interactive. The prompt goes over stdin: argv has a size limit a lecture blows through.
        let mut c = Command::new(&s.claude_bin).args(["-p", "--output-format", "json", "--dangerously-skip-permissions"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("claude: {e}"))?;
        let mut stdin = c.stdin.take().unwrap();
        let p = prompt.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(p.as_bytes());
        });
        c
    };
    on_spawn(child.id());
    let out = wait_with_timeout(child, timeout)?;
    if s.brain == "ollama" {
        return Ok(out);
    }
    // --output-format json wraps the answer: {"result": "...", "is_error": false, ...}
    let v: Value = serde_json::from_str(&out).map_err(|e| format!("claude json: {e}: {}", out.chars().take(300).collect::<String>()))?;
    if v["is_error"].as_bool().unwrap_or(false) {
        return Err(format!("claude: {}", v["result"].as_str().unwrap_or("error")));
    }
    Ok(v["result"].as_str().unwrap_or("").to_string())
}

fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<String, String> {
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let (mut o, mut e) = (String::new(), String::new());
        let eth = std::thread::spawn(move || {
            let mut e = String::new();
            let _ = std::io::BufReader::new(stderr).read_to_string(&mut e);
            e
        });
        let _ = std::io::BufReader::new(stdout).read_to_string(&mut o);
        e.push_str(&eth.join().unwrap_or_default());
        (o, e)
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (o, e) = reader.join().unwrap_or_default();
                if status.success() || !o.trim().is_empty() {
                    return Ok(o);
                }
                return Err(format!("brain exit {status}: {}", e.lines().last().unwrap_or("")));
            }
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                return Err("brain timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// The JSON array inside whatever the model wrapped it in.
pub fn parse_drafts(text: &str) -> Result<Vec<Draft>, String> {
    let start = text.find('[').ok_or("no JSON array in the answer")?;
    let end = text.rfind(']').ok_or("no JSON array in the answer")?;
    if end <= start {
        return Err("no JSON array in the answer".into());
    }
    let v: Value = serde_json::from_str(&text[start..=end]).map_err(|e| format!("reel json: {e}"))?;
    let strs = |x: &Value| x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(|t| t.trim_start_matches('#').to_string())).collect()).unwrap_or_default();
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some(Draft {
                start_seg: c["start_seg"].as_u64()? as usize,
                end_seg: c["end_seg"].as_u64()? as usize,
                category: c["category"].as_str().filter(|k| CATEGORIES.contains(k)).unwrap_or("statement").to_string(),
                score: c["score"].as_i64().unwrap_or(5).clamp(1, 10),
                title: c["title"].as_str().unwrap_or("").to_string(),
                hook: c["hook"].as_str().unwrap_or("").to_string(),
                caption: c["caption"].as_str().unwrap_or("").to_string(),
                hashtags: strs(&c["hashtags"]),
                why: c["why"].as_str().unwrap_or("").to_string(),
                translation: strs(&c["translation"]),
            })
        })
        .collect())
}

/// Drafts → candidates with real times, clamped to the max length at a
/// segment boundary, overlaps resolved by score.
pub fn to_candidates(drafts: Vec<Draft>, segments: &[Segment], source_id: &str, max_s: f64) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for d in drafts {
        if d.start_seg >= segments.len() || d.end_seg < d.start_seg {
            continue;
        }
        let mut end_seg = d.end_seg.min(segments.len() - 1);
        let start = segments[d.start_seg].start;
        while end_seg > d.start_seg && segments[end_seg].end - start > max_s {
            end_seg -= 1;
        }
        let end = segments[end_seg].end.min(start + max_s);
        if end - start < 5.0 {
            continue;
        }
        let translation = segments[d.start_seg..=end_seg]
            .iter()
            .zip(d.translation.iter())
            .map(|(g, t)| Segment { start: g.start, end: g.end, text: t.clone(), words: vec![] })
            .collect();
        out.push(Candidate {
            id: crate::model::new_id("c"),
            source_id: source_id.into(),
            start,
            end,
            category: d.category,
            score: d.score,
            title: d.title,
            hook: d.hook,
            caption: d.caption,
            hashtags: d.hashtags,
            why: d.why,
            translation,
            crop: serde_json::json!({ "x": 0.5 }),
            ..Default::default()
        });
    }
    // Overlaps: keep the higher score.
    out.sort_by(|a, b| b.score.cmp(&a.score));
    let mut kept: Vec<Candidate> = Vec::new();
    for c in out {
        if kept.iter().any(|k| c.start < k.end && k.start < c.end) {
            continue;
        }
        kept.push(c);
    }
    kept.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    for (i, c) in kept.iter_mut().enumerate() {
        c.position = i as i64;
        let t = crate::model::now();
        c.created_at = t.clone();
        c.updated_at = t;
    }
    kept
}

/// A transcript longer than ~80k characters goes in overlapping chunks.
pub fn chunks(segments: &[Segment]) -> Vec<(usize, usize)> {
    const LIMIT: usize = 80_000;
    let total: usize = segments.iter().map(|s| s.text.len() + 24).sum();
    if total <= LIMIT {
        return vec![(0, segments.len())];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < segments.len() {
        let mut j = i;
        let mut n = 0;
        while j < segments.len() && n < LIMIT {
            n += segments[j].text.len() + 24;
            j += 1;
        }
        out.push((i, j));
        if j >= segments.len() {
            break;
        }
        // Back up ~60 s so a reel on the seam is seen whole by one chunk.
        let seam = segments[j - 1].end - 60.0;
        let mut k = j - 1;
        while k > i && segments[k].start > seam {
            k -= 1;
        }
        i = k.max(i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn segs() -> Vec<Segment> {
        (0..10).map(|i| Segment { start: i as f64 * 10.0, end: i as f64 * 10.0 + 9.0, text: format!("line {i}"), words: vec![] }).collect()
    }
    #[test]
    fn drafts_become_candidates() {
        let text = "Here you go:\n[{\"start_seg\":1,\"end_seg\":6,\"category\":\"story\",\"score\":9,\"title\":\"t\",\"hook\":\"h\",\"caption\":\"c\",\"hashtags\":[\"#sabr\",\"tafsir\"],\"why\":\"w\",\"translation\":[\"a\",\"b\"]},{\"start_seg\":2,\"end_seg\":3,\"category\":\"zzz\",\"score\":4}]";
        let d = parse_drafts(text).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].hashtags, vec!["sabr", "tafsir"]);
        assert_eq!(d[1].category, "statement");
        let c = to_candidates(d, &segs(), "s", 40.0);
        // 1..6 spans 59 s → clamped to segments 1..4 (end 49) = 39 s; the overlapping weaker one is dropped.
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].start, 10.0);
        assert!(c[0].end <= 50.0);
        assert_eq!(c[0].translation.len(), 2);
        assert_eq!(chunks(&segs()), vec![(0, 10)]);
    }
}

/// A correction: in segment `seg`, the words `from` become `to` (same count).
pub struct Fix {
    pub seg: usize,
    pub from: String,
    pub to: String,
}

pub fn polish_prompt(segments: &[Segment], offset: usize, language: &str) -> String {
    let lines: Vec<String> = segments.iter().enumerate().map(|(i, g)| format!("#{} {}", i + offset, g.text.trim())).collect();
    format!(
        r##"You proofread an automatic transcript of a spoken lecture in {lang}, which becomes on-screen captions. Stay faithful to what was said: do NOT rephrase, shorten, translate or improve style. Only fix clear errors a viewer would notice: wrong verb forms (Dutch "trekte" → "trok"), misheard words that make no sense in context, misspelled names and religious terms, wrong homophones. Leave dialect, filler words, repetitions and informal grammar alone. Change at most about 5% of the words.

Each fix replaces a run of words with the same number of words, so the timing of every word stays valid. Give the exact original words as they appear (case and punctuation included).

Return ONLY a JSON array, no prose, possibly empty: [{{"seg": <line index>, "from": "<exact words in the line>", "to": "<replacement, same word count>"}}]

Lines:
{lines}"##,
        lang = lang_name(language),
        lines = lines.join("\n"),
    )
}

pub fn parse_fixes(text: &str) -> Result<Vec<Fix>, String> {
    let start = text.find('[').ok_or("no JSON array in the answer")?;
    let end = text.rfind(']').ok_or("no JSON array in the answer")?;
    if end < start {
        return Err("no JSON array in the answer".into());
    }
    let v: Value = serde_json::from_str(&text[start..=end]).map_err(|e| format!("fix json: {e}"))?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some(Fix { seg: f["seg"].as_u64()? as usize, from: f["from"].as_str()?.trim().to_string(), to: f["to"].as_str()?.trim().to_string() }))
        .filter(|f| !f.from.is_empty() && !f.to.is_empty() && f.from != f.to && f.from.split_whitespace().count() == f.to.split_whitespace().count())
        .collect())
}

fn strip_punct(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Runs of whole words (by stripped form) in `tokens`, as start indices, in order.
fn word_runs(tokens: &[String], keys: &[String]) -> Vec<usize> {
    if keys.is_empty() || tokens.len() < keys.len() {
        return vec![];
    }
    (0..=tokens.len() - keys.len()).filter(|&i| tokens[i..i + keys.len()] == keys[..]).collect()
}

/// A word with the replacement's letters and the original's punctuation around them.
fn reword(old: &str, new: &str) -> String {
    let lead: String = old.chars().take_while(|c| !c.is_alphanumeric()).collect();
    let trail: String = old.chars().rev().take_while(|c| !c.is_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{lead}{}{trail}", strip_punct_keep_case(new))
}

/// Apply fixes to the segments: the text and the timed words get the SAME
/// occurrence, matched on whole words, and a fix that cannot be placed in
/// both representations is skipped. Returns how many fixes landed.
pub fn apply_fixes(segments: &mut [Segment], fixes: &[Fix]) -> usize {
    let mut n = 0;
    for f in fixes {
        let Some(seg) = segments.get_mut(f.seg) else { continue };
        let keys: Vec<String> = f.from.split_whitespace().map(strip_punct).collect();
        let to: Vec<&str> = f.to.split_whitespace().collect();
        if keys.is_empty() || keys.len() != to.len() {
            continue;
        }
        let text_tokens: Vec<String> = seg.text.split_whitespace().map(String::from).collect();
        let text_keys: Vec<String> = text_tokens.iter().map(|w| strip_punct(w)).collect();
        // The occurrence: prefer the exact-cased one, else the first by stripped form.
        let runs = word_runs(&text_keys, &keys);
        let Some(&ti) = runs.iter().find(|&&i| text_tokens[i..i + keys.len()].iter().zip(f.from.split_whitespace()).all(|(a, b)| strip_punct_keep_case(a) == strip_punct_keep_case(b))).or(runs.first()) else { continue };
        let ordinal = runs.iter().position(|&i| i == ti).unwrap_or(0);
        // The same ordinal occurrence among the timed words, when there are any.
        let wi = if seg.words.is_empty() {
            None
        } else {
            let word_keys: Vec<String> = seg.words.iter().map(|w| strip_punct(&w.w)).collect();
            let wruns = word_runs(&word_keys, &keys);
            match wruns.get(ordinal).or(wruns.last()) {
                Some(&i) => Some(i),
                None => continue, // present in the text but not in the timed words: leave both alone
            }
        };
        let mut tokens = text_tokens;
        for (k, w) in to.iter().enumerate() {
            tokens[ti + k] = reword(&tokens[ti + k], w);
        }
        seg.text = tokens.join(" ");
        if let Some(i) = wi {
            for (k, w) in to.iter().enumerate() {
                seg.words[i + k].w = reword(&seg.words[i + k].w, w);
            }
        }
        n += 1;
    }
    n
}

fn strip_punct_keep_case(w: &str) -> &str {
    w.trim_matches(|c: char| !c.is_alphanumeric())
}

#[cfg(test)]
mod polish_tests {
    use super::*;
    use crate::model::Word;
    #[test]
    fn fixes_keep_timing() {
        let mut segs = vec![Segment { start: 0.0, end: 2.0, text: "En Allah trekte hun aandacht.".into(), words: vec![Word { w: "En".into(), s: 0.0, e: 0.3 }, Word { w: "Allah".into(), s: 0.3, e: 0.7 }, Word { w: "trekte".into(), s: 0.7, e: 1.1 }, Word { w: "hun".into(), s: 1.1, e: 1.4 }, Word { w: "aandacht.".into(), s: 1.4, e: 2.0 }] }];
        let fixes = parse_fixes(r#"ok [{"seg":0,"from":"trekte","to":"trok"},{"seg":0,"from":"hun aandacht","to":"de aandacht van iedereen"},{"seg":5,"from":"x","to":"y"}]"#).unwrap();
        assert_eq!(fixes.len(), 2); // the word-count change is refused; the out-of-range one is dropped on apply
        assert_eq!(apply_fixes(&mut segs, &fixes), 1);
        assert_eq!(segs[0].text, "En Allah trok hun aandacht.");
        assert_eq!(segs[0].words[2].w, "trok");
        assert_eq!(segs[0].words[2].s, 0.7);
        // Same occurrence in both: "Hun … hun" with a fix on the second "hun".
        let mut segs = vec![Segment { start: 0.0, end: 2.0, text: "Hun boek, hun keuze.".into(), words: ["Hun", "boek,", "hun", "keuze."].iter().enumerate().map(|(i, w)| Word { w: w.to_string(), s: i as f64 * 0.5, e: i as f64 * 0.5 + 0.4 }).collect() }];
        let fixes = parse_fixes(r#"[{"seg":0,"from":"hun keuze","to":"zijn keuze"}]"#).unwrap();
        assert_eq!(apply_fixes(&mut segs, &fixes), 1);
        assert_eq!(segs[0].text, "Hun boek, zijn keuze.");
        assert_eq!(segs[0].words[0].w, "Hun");
        assert_eq!(segs[0].words[2].w, "zijn");
        // A substring is not a word: "hun" must not touch "hunkering".
        let mut segs = vec![Segment { start: 0.0, end: 1.0, text: "hunkering naar".into(), words: vec![Word { w: "hunkering".into(), s: 0.0, e: 0.5 }, Word { w: "naar".into(), s: 0.5, e: 1.0 }] }];
        assert_eq!(apply_fixes(&mut segs, &parse_fixes(r#"[{"seg":0,"from":"hun","to":"zijn"}]"#).unwrap()), 0);
        assert_eq!(segs[0].text, "hunkering naar");
    }
}
