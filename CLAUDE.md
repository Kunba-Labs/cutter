# Cuttar

Native macOS app that turns long Islamic lectures into reels and posts them, and
makes the weekly lesson poster plus its "starting soon" video. Rust core + Tauri,
patterned on `~/Development/membox` (dispatch core, loopback MCP, dev/prod
identities, local installer). Plan and decisions: `docs/PLAN.md`. Design canvas
(all screens): https://claude.ai/artifact/1ENrW18nVFDjWmiXPfhsnp

## Layout

```
core/      cuttar-core: SQLite document store (db.rs), models (model.rs), jobs + scheduler
           (pipeline.rs), yt-dlp (ytdlp.rs), whisper via mlx_whisper CLI (whisper.rs),
           reel finding through `claude -p` or ollama (brain.rs), ASS captions (captions.rs),
           ffmpeg render / covers / waiting loop (ffmpeg.rs), YouTube OAuth + resumable
           upload (youtube.rs), posters via Claude+Higgsfield MCP + local QR (posters.rs),
           loopback HTTP: /mcp (MCP), /dispatch (CLI), /health (mcp.rs).
           ONE write entry point: Library::dispatch(action, json) — same names in JS, CLI, MCP.
cli/       `cuttar`: talks to the running app over /dispatch; `cuttar headless` runs the core
           without a window; `cuttar mcp` is a stdio bridge; `cuttar mcp-url` prints the endpoint.
desktop/   Tauri v2 + React 19 + Vite, plain CSS (src/app.css = the design tokens).
           src/store.js = invoke("dispatch") + snapshot on `library-changed`.
tools/     add_qr.py (pastes + verifies the QR into a poster's white panel; embedded in the core).
bin/       dev (Cuttar Dev identity), smoke (end-to-end check on a scratch library).
```

## Commands

```sh
bin/dev                              # "Cuttar Dev" (com.cuttar.desktop.dev, port 47252) — the play identity
bin/app-build ["string in binary"]   # release bundle that really embeds the current frontend (touches main.rs; cargo misses dist changes)
bin/app-restart                      # relaunch the bundle once the queue is idle
cd desktop && yarn install:app       # build, sign, install /Applications/Cuttar.app (com.cuttar.desktop, port 47251)
cd desktop && yarn install:app:dev   # same for "Cuttar Dev"
cargo test -p cuttar-core            # the tests that matter
bin/smoke                            # synthetic lecture through the whole pipeline (needs claude, ffmpeg, mlx_whisper)
cargo build --release -p cuttar && ln -sf $PWD/target/release/cuttar ~/.local/bin/cuttar
```

Data: `~/Library/Application Support/com.cuttar.desktop[.dev]/` — `cuttar.sqlite`, `cuttar.log`,
`mcp-token`, `mcp.json` (the `claude mcp add` line), `add_qr.py`. Output: `~/Movies/Cuttar/<source>/<reel>/<format>.mp4`
(+ cover, captions.srt, caption.txt), `~/Movies/Cuttar/posters/<event>/`.

## Decisions that must hold

- **One process owns the database.** The app (or `cuttar headless`) runs the workers, the
  scheduler and the HTTP server on a fixed port; the CLI and Claude go through it. Never open
  cuttar.sqlite from a second process.
- **Reels are complete thoughts, aimed at `settings.target_reel_s` (60), never past
  `max_reel_s` (90; a stored limit below the target is lifted to 1.5×).** The brain gets segment
  indices, never timestamps, and must keep the whole point without padding. `brain::to_candidates`
  DROPS a draft over the max instead of cutting it (a clamped reel loses its point). The trimmer
  refuses a longer cut; Trim has ± line / ± 1 s at both edges over a ±15 s context strip.
- **Teaser first.** Each reel opens with its punchline (`candidate.punch_start/end`, found by the
  brain as `punch_seg` + a verbatim quote located on word timings, `brain::punch_times`, ≤ 8 s),
  then `ffmpeg::TRANSITIONS` (xfade + a pink-noise whoosh), then the reel. The title rides on the
  teaser. `settings.intro` / `candidate.intro_on`, `transition`; `punchline` job asks again.
  One filter graph in `ffmpeg::render`: teaser+cut (own ASS each) → xfade → logo → loudnorm → end card.
  `pipeline::plan` decides what a file gets and its length; verify_renders uses the same plan.
- **Every reel also renders `clean.mp4`** (`reel_formats`): the bare cut, no captions, title,
  teaser, logo or end card, no cover. captions.srt matches it.
- **Logo**: `settings.logo` (path, size, opacity, default place); `candidate.logo` {on,x,y} per
  reel, dragged in the preview. x/y place it within the free space (0..1), same maths both sides.
- **Nothing posts without a confirm** unless a channel says `auto_schedule`. YouTube uploads
  as private with `publishAt`; TikTok/Meta are not linked (need approved apps): a confirmed post
  there fails with the file path + caption.txt for a manual upload. Messy is out (Waseem, 2026-09-26).
- **Transcripts keep word timings** (whisper). YouTube captions are cue-level; karaoke then
  spreads words evenly over the cue.
- **Posters follow the tafsir-flyer recipe**: 3 variants per run, palette history to avoid
  repeats, blank white panel → real QR pasted and decoded locally. Generation runs `claude -p`
  with the Higgsfield MCP already in Claude Code; the app holds no Higgsfield key.
- **Quality before speed.** yt-dlp takes the best stream YouTube has (any resolution, VP9/AV1;
  AAC audio when offered). Renders use x264 `slow` crf 16 by default (`settings.render_quality`:
  best | good | fast) at the source frame rate, capped at 60. A source the in-app player cannot
  play (VP9, AV1, above 1080p, Opus) gets `preview.mp4`, a 1080p H.264 copy; the UI plays
  `previewPath || videoPath`, renders read the original. `redownload` refetches and keeps
  transcript, reels and edits (old file parked as `source.prev.mp4` until the new one lands).
- **Covers are chosen, not taken at +1 s.** The first render of a reel tiles nine numbered frames
  of the cut (`ffmpeg::cover_sheet`), the brain reads the sheet (both claude and ollama take a file
  path in the prompt) and answers `{"frame": n}`. The time lands on `candidate.cover_t`; other
  formats reuse it, a later render job of the same reel waits instead of asking again. The sheet
  stays beside the reel as `cover-sheet.jpg`. Fallback on any error: one second in. The nine
  frames are the sharpest in nine windows of the cut (`ffmpeg::sharp_times`, mean Sobel edge
  strength at 4 fps over the crop). The cover carries the title at 1.5× the reel's title size
  (`captions::cover`). `covers` redoes cover images without re-encoding; `pick_cover` forgets the
  frame and renders the reel again ("New cover" on the Reels screen).
- **Post targets, not channels.** `targets` table: kind youtube | instagram | tiktok | facebook |
  folder, own name, hours, format, auto_schedule; a YouTube target holds its own login (the OAuth
  client in `settings.youtube` is shared, and built in). `post.target_id` says where a post goes,
  `candidate.targets` (empty = all) which targets a reel is offered to; `autofill` walks targets.
  A folder target copies file + cover + caption into a drop folder at post time; Instagram/TikTok/
  Facebook are "by hand" until an approved developer app exists. Watched channels (`channels`)
  are the ingest side and unrelated. `settings.cadence` is legacy, unused.
- **Design language**: panels on #1A1D2A with 26 px header strips, magenta #F52ACB the only
  action colour (primary buttons, active tab). Tiffany #0ABAB5 for selection state: segmented
  controls, checks, slider thumbs, links, inspector group heads. Mint = done, the trim timeline
  and the current word; coral = failed. No stat tiles, no caps labels, no pill nav.

## Gotchas

- libass: BorderStyle 3 (opaque box) is drawn in the OUTLINE colour, padded by the Outline width;
  BackColour is only the shadow. Outline 0 means no box at all.
- Caption fonts: every name in store.js `FONTS` was checked to resolve in libass (`-loglevel verbose`
  shows `fontselect:`). Six OFL fonts ship in desktop/src/fonts; the core `include_bytes!`s them into
  `<data>/fonts` and every `ass=` goes through `ffmpeg::ass_filter` (adds `fontsdir`). One-weight faces
  (`FIXED_FONTS`, both sides) are never bolded. The preview asks 700/400 like libass, not 800/500.
- libass sizes a font by its line height, CSS by its em: captions::build multiplies sizes by
  EM = 1.18 so a render matches the preview (Helvetica Bold). Other fonts differ a little.

- The webview loads local files through `media://` (our handler in `src-tauri/src/main.rs`), not `asset://`: Tauri's asset protocol caps a range at 1 MB and WebKit then can't read a movie index larger than that (long 60 fps lectures: black picture, audio plays). `fileUrl` in store.js picks the scheme.

- Never pipe `bin/app-build` into `tail`: the pipe hides its exit code and a missing literal (stale dist) slips through. Redirect to a file and check `$?`.

- A new document table must be listed in `db::TABLES`, or `put` fails quietly and `all` is empty.

- A GUI app gets launchd's PATH: `tools::inherit_login_path()` runs first in both binaries.
- yt-dlp: `--sub-langs xx.*` gets 429'd; ask for `xx,xx-orig,xx-xx`. `--dump-single-json` line
  is the last `{` line of stdout regardless of exit code.
- mlx_whisper prints `[mm:ss.mmm --> mm:ss.mmm]` per segment in verbose mode — that is the progress.
- `claude -p` reads the prompt from stdin (argv has a size limit); `--output-format json` wraps
  the answer in `{"result": …}`. A raw string holding `"#` needs `r##"…"##`.
- libass: the `ass=` filter path must escape `:` and `\`; ASS colours are `&HAABBGGRR`.
- ffmpeg drawtext countdown: `%{eif:…}` with `\:` and `\,` escaped inside the filter.
- Tauri `dragDropEnabled` is false so the webview gets its own drops.
- Release profile has no `lto` and no `strip`: on this toolchain (rustc 1.96, Xcode 27) `lto = true` corrupts proc-macro dylibs (darling, ctor: "mis-aligned LINKEDIT string pool", surfacing as E0463). `cargo clean --release` after changing the profile.
- The desktop executable is `cuttar-desktop` (tauri.conf `mainBinaryName`), never `Cuttar`: on the case-insensitive disk that name overwrote `target/release/cuttar`, the CLI, and every CLI call launched a second app.
- Verify a frontend change in `desktop/dist/assets/*.js`, never by grepping the binary: Tauri embeds the assets brotli-compressed. `bin/app-build "literal"` does that check. Edit scripts that assert on old text abort BEFORE writing; check the source (`git status`) before building.
- Two processes must never write the same output file: cancel or wait for a job before queueing the same one with different args (`enqueue` only dedupes identical args).
