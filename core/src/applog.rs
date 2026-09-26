//! The log file. A GUI app's stderr goes nowhere, so every `log::` call also
//! lands in `<data dir>/cuttar.log` (one rotation at 8 MB). Lifted from membox.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub struct FileLog {
    file: Mutex<Option<File>>,
    level: log::LevelFilter,
}

pub fn init(dir: &Path, level: log::LevelFilter) -> PathBuf {
    let path = dir.join("cuttar.log");
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("cuttar.log.1"));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    log::set_max_level(level);
    let _ = log::set_boxed_logger(Box::new(FileLog { file: Mutex::new(file), level }));
    log::info!("--- cuttar {} starting ---", env!("CARGO_PKG_VERSION"));
    path
}

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= self.level
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!("{} {:<5} {} {}\n", crate::model::now(), r.level(), r.target(), r.args());
        eprint!("{line}");
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }
    fn flush(&self) {
        if let Some(f) = self.file.lock().as_mut() {
            let _ = f.flush();
        }
    }
}
