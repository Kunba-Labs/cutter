//! Where the sidecar tools are, and the login-shell PATH a GUI app is denied.

use std::process::Command;

use crate::model::Tools;

/// A GUI app launched from Finder inherits launchd's stub PATH; ask the login
/// shell for the real one once (membox's fix, verbatim in spirit).
pub fn inherit_login_path() {
    let current = std::env::var("PATH").unwrap_or_default();
    if current.split(':').filter(|s| !s.is_empty()).count() > 5 {
        return;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let Ok(out) = Command::new(shell).args(["-lc", "printf %s \"$PATH\""]).output() else { return };
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.split(':').filter(|s| !s.is_empty()).count() > current.split(':').count() {
        std::env::set_var("PATH", path);
    }
}

/// Homebrew's plain `ffmpeg` formula has no libass since 9.0; `ffmpeg-full` is
/// keg-only, so prefer it when installed. Same for ffprobe.
pub fn ffmpeg_bin() -> String {
    for p in ["/opt/homebrew/opt/ffmpeg-full/bin/ffmpeg", "/usr/local/opt/ffmpeg-full/bin/ffmpeg"] {
        if std::path::Path::new(p).exists() {
            return p.into();
        }
    }
    "ffmpeg".into()
}

pub fn ffprobe_bin() -> String {
    let f = ffmpeg_bin();
    match f.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/ffprobe"),
        None => "ffprobe".into(),
    }
}

fn find(bin: &str) -> Option<String> {
    which::which(bin).ok().map(|p| p.display().to_string())
}

pub fn detect(claude_bin: &str) -> Tools {
    let ffmpeg_ass = Command::new(ffmpeg_bin()).args(["-hide_banner", "-filters"]).output().map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.split_whitespace().nth(1) == Some("ass"))).unwrap_or(false);
    Tools {
        ffmpeg: if ffmpeg_bin() == "ffmpeg" { find("ffmpeg") } else { Some(ffmpeg_bin()) },
        ffmpeg_ass,
        ytdlp: find("yt-dlp"),
        whisper: find("mlx_whisper"),
        claude: find(claude_bin),
        ollama: find("ollama"),
        uv: find("uv"),
    }
}

/// `uv tool install mlx-whisper` — the model itself downloads on first use.
pub fn install_whisper() -> Result<String, String> {
    let out = Command::new("uv").args(["tool", "install", "--force", "mlx-whisper"]).output().map_err(|e| format!("uv: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() { Ok(text) } else { Err(text) }
}

/// Run a command, capture both streams, fail on a non-zero exit with stderr.
pub fn run(cmd: &mut Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{:?} exit {}: {}", cmd.get_program(), out.status, err.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ")))
    }
}
