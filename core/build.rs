//! The YouTube OAuth client secret comes from the deploy environment, never from the repo:
//! `CUTTAR_YT_SECRET` in the environment, else in `.deploy.env` at the repo root (gitignored).
//! Missing: the build still works and "Connect" says what to set.

use std::path::Path;

fn main() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.deploy.env");
    println!("cargo:rerun-if-env-changed=CUTTAR_YT_SECRET");
    println!("cargo:rerun-if-changed={}", file.display());
    let from_file = std::fs::read_to_string(&file).ok().and_then(|s| {
        s.lines().filter_map(|l| l.trim().strip_prefix("CUTTAR_YT_SECRET=")).map(|v| v.trim().trim_matches('"').to_string()).next()
    });
    let secret = std::env::var("CUTTAR_YT_SECRET").ok().filter(|v| !v.is_empty()).or(from_file).unwrap_or_default();
    println!("cargo:rustc-env=CUTTAR_YT_SECRET={secret}");
}
