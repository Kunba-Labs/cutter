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
- **Reels are complete thoughts, ≤ `settings.max_reel_s` (40).** The brain gets segment
  indices, never timestamps; `brain::to_candidates` clamps at a segment boundary. The trimmer
  refuses a longer cut.
- **Nothing posts without a confirm** unless a channel says `auto_schedule`. YouTube uploads
  as private with `publishAt`; TikTok/Meta are not linked (need approved apps): a confirmed post
  there fails with the file path + caption.txt for a manual upload. Messy is out (Waseem, 2026-09-26).
- **Transcripts keep word timings** (whisper). YouTube captions are cue-level; karaoke then
  spreads words evenly over the cue.
- **Posters follow the tafsir-flyer recipe**: 3 variants per run, palette history to avoid
  repeats, blank white panel → real QR pasted and decoded locally. Generation runs `claude -p`
  with the Higgsfield MCP already in Claude Code; the app holds no Higgsfield key.
- **Design language**: panels on #1A1D2A with 26 px header strips, magenta #F52ACB the only
  action colour, mint = done, coral = failed. No stat tiles, no caps labels, no pill nav.

## Gotchas

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
- Two processes must never write the same output file: cancel or wait for a job before queueing the same one with different args (`enqueue` only dedupes identical args).
