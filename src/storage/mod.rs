//! Storage Layer — plain-text persistence for user progress.
//!
//! This module is the only place that knows both `progress::` types and
//! the on-disk format at the same time (the same role `content::` plays
//! for engine + JSON). When SQLite replaces plain text later, only this
//! module changes.
//!
//! File layout: one file per language at
//!   $XDG_CONFIG_HOME/termocab/progress/<lang>.txt   (or ~/.config/... if
//!   XDG_CONFIG_HOME is unset)
//!
//! File format (human-readable, diff-friendly, line-oriented):
//!   - blank lines and lines starting with '#' are ignored
//!   - `stage <id>`  marks a stage as completed
//!   - `day <YYYY-MM-DD>`  records an active day
//!   - anything else is a parse error — strict on purpose, so a format
//!     change fails loudly instead of silently dropping history

use crate::progress::LanguageProgress;
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Parse(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "could not read or write progress file: {e}"),
            StoreError::Parse(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Per-language progress file path under the config directory.
pub fn progress_path(lang: &str) -> PathBuf {
    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home_dir().join(".config"),
    };
    base.join("termocab").join("progress").join(format!("{lang}.txt"))
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Loads progress for a language. A missing file is not an error — it
/// just means the learner is brand new — so this returns fresh progress
/// instead of failing.
pub fn load(lang: &str) -> Result<LanguageProgress, StoreError> {
    let path = progress_path(lang);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LanguageProgress::new());
        }
        Err(e) => return Err(StoreError::Io(e)),
    };

    let mut completed = Vec::new();
    let mut days = Vec::new();
    for (no, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        match (parts.next(), parts.next()) {
            (Some("stage"), Some(id)) => completed.push(id
                .trim()
                .parse::<u32>()
                .map_err(|_| StoreError::Parse(format!("{}: line {}: bad stage id: {id:?}", path.display(), no + 1)))?),
            (Some("day"), Some(day)) => days.push(day.trim().to_string()),
            _ => {
                return Err(StoreError::Parse(format!(
                    "{}: line {}: unrecognized line: {line:?}",
                    path.display(),
                    no + 1
                )));
            }
        }
    }
    Ok(LanguageProgress::from_parts(completed, days))
}

/// Saves progress for a language. Written to a sibling temp file first
/// and renamed into place, so a crash mid-write can't destroy history.
pub fn save(lang: &str, progress: &LanguageProgress) -> Result<(), StoreError> {
    let path = progress_path(lang);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(StoreError::Io)?;
    }

    let mut text = String::from("# termocab progress\n# stage <id> = completed stage; day <date> = active day\n");
    for id in progress.completed_stages() {
        text.push_str(&format!("stage {id}\n"));
    }
    for day in progress.active_days() {
        text.push_str(&format!("day {day}\n"));
    }

    let tmp = path.with_extension("txt.tmp");
    fs::write(&tmp, text).map_err(StoreError::Io)?;
    fs::rename(&tmp, &path).map_err(StoreError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// All three tests mutate XDG_CONFIG_HOME, and cargo runs tests in
    /// parallel threads — this lock makes the tests that touch the env
    /// run one at a time.
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// Holds XDG_CONFIG_HOME stable for the whole test body.
    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("termocab-test-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_through_disk() {
        let _guard = lock_env();
        let dir = scratch("roundtrip");
        std::env::set_var("XDG_CONFIG_HOME", &dir);

        let mut p = LanguageProgress::new();
        p.record_stage_completion(1, "2026-08-21");
        p.record_stage_completion(2, "2026-08-22");
        save("french", &p).unwrap();

        let loaded = load("french").unwrap();
        assert_eq!(loaded.completed_stages(), vec![1, 2]);
        assert_eq!(loaded.streak_on("2026-08-22"), 2);
    }

    #[test]
    fn missing_file_is_fresh_progress() {
        let _guard = lock_env();
        let dir = scratch("missing");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        assert_eq!(load("klingon").unwrap().completed_stages(), Vec::<u32>::new());
    }

    #[test]
    fn unknown_line_is_an_error() {
        let _guard = lock_env();
        let dir = scratch("strict");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        fs::create_dir_all(dir.join("termocab/progress")).unwrap();
        fs::write(
            dir.join("termocab/progress/french.txt"),
            "stage 1\nfrobnicate 7\n",
        )
        .unwrap();
        assert!(matches!(load("french"), Err(StoreError::Parse(_))));
    }
}
