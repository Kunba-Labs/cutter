# Cuttar — plan

Native macOS app (SwiftUI) that turns long Islamic lectures into reels and posts them, and makes the weekly lesson poster plus its "starting soon" video. Everything local except the calls you link.

Design canvas (all screens): https://claude.ai/artifact/1ENrW18nVFDjWmiXPfhsnp

## Pipeline

1. **Ingest** — YouTube link, watched channel/playlist (rules: interval, min length, title match, since date), or local file. yt-dlp best 1080p + captions. Listing only until a rule or the user says go.
2. **Transcribe** — language auto-detect with override; source = YouTube captions, local whisper (mlx-whisper large-v3-turbo, word timestamps), or both. Glossary. Editable, synced to player. SRT/VTT import.
3. **Find reels** — `claude -p` over the timed transcript in chunks. Complete thoughts only, ≤ 40 s, snapped to sentence/word bounds and silence. Categories fact · statement · hook · story · dua · reminder · Q&A, score 1–10 + why, title, hook, caption, hashtags. Offline fallback: ollama gemma4:26b-mlx.
4. **Review & style** — instant preview per candidate, format switch with safe zones (Shorts/Reels/TikTok 9:16, Feed 4:5, 16:9), word-handle trim, face-tracked (Vision) or manual crop, burned captions (karaoke, clean, boxed, RTL for AR/UR, translated second line), hook, logo, end card. Tick to approve, bulk by score.
5. **Render** — ffmpeg (h264_videotoolbox, CRF-equivalent quality, AAC 192k, loudnorm −14 LUFS), one job per reel × format, sidecars cover.jpg/.srt/caption.txt. Queue with progress/pause/retry/log.
6. **Publish** — YouTube Data API (Shorts, publishAt) first; TikTok Content Posting API and Meta Graph API (Reels) once their apps are approved, until then export + caption.txt. Cadence rules per channel, week calendar, delivery log. No Messy.

**Posters** — event templates (weekly tafsir etc.), 3 Higgsfield variants per run (speaker photo reference, palette history to avoid repeats), blank white panel → real QR pasted + decoded locally (`~/Development/tools/tafsir-flyer/add_qr.py` logic), exports print 2:3 / JPEG 1600 / feed 4:5 / story 9:16.

**Waiting video** — from the chosen poster: 16:9 (YouTube Live) and 9:16 (IG Live) loop, burned countdown to start time or live-clock variant, Ken Burns via ffmpeg (optional Higgsfield animated), "we zijn zo terug" and "afgelopen" stills, goes into the publish calendar.

**Autopilot** — new upload → transcript → reels → auto-approve ≥ 8 → render → schedule, with a "hold for review" gate per channel. macOS notifications, menu bar status.

## Storage

- `~/Library/Application Support/Cuttar/cuttar.sqlite` — sources, channels, transcripts (+FTS), candidates, renders, posts, posters, jobs.
- `~/Movies/Cuttar/<source>/<reel>/<format>.mp4` and `~/Movies/Cuttar/posters/<event>/`.

## Screens (1440×900)

Library · Add source sheet · Inbox (watched channels) · Transcript · Reels review · Captions & style · Queue · Publish calendar · Settings (channels, engines) · First run · Posters · Waiting video.

## Design language

Pro-editor panels on deep navy (#1A1D2A), 26 px header strips, sentence-case titles, 11.5–12 px SF Pro, 4–6 px radii. Magenta #F52ACB = the one action colour (active tab, primary, playhead, running). Mint #3EF2A3 done, coral #FF5C7A failed, cyan info. No stat tiles, no caps labels, no pill nav (Waseem's feedback 2026-09-26: the first pass looked AI-generated because of those).

## Decisions log

- 2026-09-26 — Project named **Cuttar** (folder renamed from cutter). Design-first: no code until the canvas is approved.
- 2026-09-26 — Transcription = mlx-whisper large-v3-turbo (nothing was installed; Apple SpeechAnalyzer lacks Urdu/Arabic coverage). Reel brain = claude CLI; ollama as offline fallback.
- 2026-09-26 — Publish backends: YouTube Data API first; TikTok and Meta direct once their apps are approved (manual export until then). **Messy integration dropped** on Waseem's instruction, so Facebook/Instagram posting is unsupported until the Meta app exists.
- 2026-09-26 — Stack: Rust core + Tauri (patterned on membox/inbox2), SQLite document tables, `cuttar` CLI over the app's loopback HTTP, MCP server for Claude Code. Build started the same day.
- 2026-09-26 — Mock imagery generated with Higgsfield nano_banana (9 images, ~credits from 241 → see balance); assets stored on the canvas.

## Decisions 2026-09-28 (reel editing round)

- Reel length: aim 60 s (setting + a "~N s" picker by Find again), hard max 1.5× (90 s). Over-max drafts are dropped, not truncated; prompt forbids padding with unrelated lines. Replaces the 40 s cap.
- Teaser: punchline (brain-chosen, editable: Cmd-click words, playhead, "Ask Claude") plays first, then a transition (swoosh default; zoom, slide, blur, flash, fade, cut), then the reel. On by default; per reel on/off.
- Clean cut: every reel also renders clean.mp4 (no captions/title/teaser/logo/end card).
- Logo: one library logo, per-reel on/off and drag-to-place; size/opacity global.
- End card: was only under Posters/Settings; now also on the Reels inspector, and any image can be the card (padded to 9:16, 4:5, 16:9).
- Fonts: six OFL display fonts bundled (Montserrat, Poppins, Anton, Bebas Neue, Archivo Black, Lilita One) + six new templates; preview weights fixed to match libass.

- Background music (same day): YouTube videos/playlists as sources, expanded and downloaded; per reel random (stable) / a specific track / none, own level. Default loudness raised from −14 to −11 LUFS, per reel overridable; "Apply to the queue" copies music and loudness to all reels.

## Open questions for Waseem

- Waiting video: burned countdown (render on the day) vs live clock in OBS — default burned.
- Cadence defaults per channel (mock assumes 1/day 18:00 FB+IG, Mon/Wed/Fri 17:00 Shorts).
- Whether posters keep going through Claude Code + Higgsfield MCP, or the app calls Higgsfield's API directly (needs a key).
