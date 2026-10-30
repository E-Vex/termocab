//! Storage Layer — plain-text persistence for user progress and srs state.
//!
//! This module is the only place that knows both the `progress::` and
//! `srs::` domain types AND their on-disk formats at the same time (the
//! same role `content::` plays for engine + JSON). When SQLite replaces
//! plain text later, only this module changes.
//!
//! File layout: one file per language under the config directory
//!   <config root>/progress/<lang>.txt
//!   <config root>/srs/<lang>.txt
//! The config root itself is decided by the caller — `cli/` resolves
//! $XDG_CONFIG_HOME/termocab (or ~/.config/termocab) from the
//! environment and hands it in; this module never reads the
//! environment, so it is testable with a plain directory path.
//!
//! Both formats are human-readable, diff-friendly, line-oriented:
//!   - blank lines and lines starting with '#' are ignored
//!   - progress: `stage <id>` marks a stage complete; `day <YYYY-MM-DD>`
//!     records an active day
//!   - srs: `word <word> box <1-6> due <YYYY-MM-DD>` schedules one word
//!   - anything else is a parse error — strict on purpose, so a format
//!     change fails loudly instead of silently dropping history

use crate::lang::LangId;
use crate::progress::LanguageProgress;
use crate::srs::{self, SrsState};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Parse(String),
    /// Neither XDG_CONFIG_HOME nor HOME is set: there is nowhere to put
    /// the config directory, and writing into the current directory
    /// instead would be a surprise, not a fallback.
    NoConfigDir,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "could not read or write progress file: {e}"),
            StoreError::Parse(msg) => write!(f, "{msg}"),
            StoreError::NoConfigDir => write!(
                f,
                "could not determine the config directory: XDG_CONFIG_HOME and HOME are both unset"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

/// The config root, given the two environment values the caller read:
/// $XDG_CONFIG_HOME/termocab, or ~/.config/termocab when XDG_CONFIG_HOME
/// is unset. With neither set there is no honest answer — error instead
/// of quietly writing into the current directory (item 10). Pure: no
/// env access, so this is unit-testable directly (item 16).
pub fn config_root(
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, StoreError> {
    let base = match xdg_config_home.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => match home.filter(|home| !home.is_empty()) {
            Some(home) => PathBuf::from(home).join(".config"),
            None => return Err(StoreError::NoConfigDir),
        },
    };
    Ok(base.join("termocab"))
}

/// Per-language progress file path under the given config root.
fn progress_path(root: &Path, lang: &LangId) -> PathBuf {
    root.join("progress").join(format!("{}.txt", lang.as_str()))
}

/// Per-language srs state file path under the given config root.
fn srs_path(root: &Path, lang: &LangId) -> PathBuf {
    root.join("srs").join(format!("{}.txt", lang.as_str()))
}

/// Loads progress for a language. A missing file is not an error — it
/// just means the learner is brand new — so this returns fresh progress
/// instead of failing.
pub fn load(root: &Path, lang: &LangId) -> Result<LanguageProgress, StoreError> {
    let path = progress_path(root, lang);
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
            (Some("stage"), Some(id)) => {
                completed.push(id.trim().parse::<u32>().map_err(|_| {
                    StoreError::Parse(format!(
                        "{}: line {}: bad stage id: {id:?}",
                        path.display(),
                        no + 1
                    ))
                })?)
            }
            (Some("day"), Some(day)) => {
                let day = day.trim();
                // A malformed date would make prev_day return None and the
                // streak silently become 0 — reject it here, loudly, like
                // every other bad line (review item 13).
                if !srs::valid_date(day) {
                    return Err(StoreError::Parse(format!(
                        "{}: line {}: bad date (expected a real YYYY-MM-DD day): {day:?}",
                        path.display(),
                        no + 1
                    )));
                }
                days.push(day.to_string());
            }
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
pub fn save(root: &Path, lang: &LangId, progress: &LanguageProgress) -> Result<(), StoreError> {
    let path = progress_path(root, lang);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(StoreError::Io)?;
    }

    let mut text = String::from(
        "# termocab progress\n# stage <id> = completed stage; day <date> = active day\n",
    );
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

/// Loads the srs state for a language. A missing file is a brand-new
/// schedule, not an error — same stance as progress.
///
/// One line per card:
///   `word <word> box <1-6> due <YYYY-MM-DD>`
/// The word itself may contain spaces (a vocabulary entry is not
/// guaranteed to be a single token), so the line is split on the fixed
/// ` box ` and ` due ` markers rather than on whitespace.
pub fn load_srs(root: &Path, lang: &LangId) -> Result<SrsState, StoreError> {
    let path = srs_path(root, lang);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SrsState::new());
        }
        Err(e) => return Err(StoreError::Io(e)),
    };

    let mut entries = Vec::new();
    for (no, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rest = line
            .strip_prefix("word ")
            .ok_or_else(|| bad_line(&path, no + 1, line))?;
        let (word, tail) = rest
            .split_once(" box ")
            .ok_or_else(|| bad_line(&path, no + 1, line))?;
        let (b, d) = tail
            .split_once(" due ")
            .ok_or_else(|| bad_line(&path, no + 1, line))?;
        if word.is_empty() {
            return Err(StoreError::Parse(format!(
                "{}: line {}: empty word: {line:?}",
                path.display(),
                no + 1
            )));
        }
        let box_number = b.parse::<u8>().map_err(|_| {
            StoreError::Parse(format!(
                "{}: line {}: bad box number: {b:?}",
                path.display(),
                no + 1
            ))
        })?;
        if !(1..=srs::MAX_BOX).contains(&box_number) {
            return Err(StoreError::Parse(format!(
                "{}: line {}: box number out of range 1-{}: {box_number}",
                path.display(),
                no + 1,
                srs::MAX_BOX
            )));
        }
        if !srs::valid_date(d) {
            return Err(StoreError::Parse(format!(
                "{}: line {}: bad due date: {d:?}",
                path.display(),
                no + 1
            )));
        }
        entries.push((word.to_string(), box_number, d.to_string()));
    }
    Ok(SrsState::from_parts(entries))
}

fn bad_line(path: &std::path::Path, no: usize, line: &str) -> StoreError {
    StoreError::Parse(format!(
        "{}: line {}: unrecognized line: {line:?}",
        path.display(),
        no
    ))
}

/// Saves the srs state for a language, temp-file + rename like progress.
/// Cards are written in word order, so diffs stay small.
pub fn save_srs(root: &Path, lang: &LangId, state: &SrsState) -> Result<(), StoreError> {
    let path = srs_path(root, lang);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(StoreError::Io)?;
    }

    let mut text = String::from("# termocab srs\n# word <word> box <1-6> due <YYYY-MM-DD>\n");
    for (word, card) in state.cards() {
        text.push_str(&format!(
            "word {} box {} due {}\n",
            word, card.box_number, card.due
        ));
    }

    let tmp = path.with_extension("txt.tmp");
    fs::write(&tmp, text).map_err(StoreError::Io)?;
    fs::rename(&tmp, &path).map_err(StoreError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testenv::TempDir;

    /// A valid LangId for tests — "french" unless said otherwise.
    fn lang(s: &str) -> LangId {
        LangId::parse(s).unwrap()
    }

    #[test]
    fn config_root_prefers_xdg_then_home_then_errors() {
        // XDG_CONFIG_HOME wins when set and non-empty.
        assert_eq!(
            config_root(Some("/xdg"), Some("/home")).unwrap(),
            PathBuf::from("/xdg/termocab")
        );
        // HOME/.config is the fallback.
        assert_eq!(
            config_root(None, Some("/home")).unwrap(),
            PathBuf::from("/home/.config/termocab")
        );
        // An empty value counts as unset.
        assert_eq!(
            config_root(Some(""), Some("/home")).unwrap(),
            PathBuf::from("/home/.config/termocab")
        );
        // Neither set: an error, never the current directory (item 10).
        assert!(matches!(
            config_root(None, None),
            Err(StoreError::NoConfigDir)
        ));
        assert!(matches!(
            config_root(Some(""), None),
            Err(StoreError::NoConfigDir)
        ));
        assert!(matches!(
            config_root(None, Some("")),
            Err(StoreError::NoConfigDir)
        ));
    }

    #[test]
    fn round_trips_through_disk() {
        let root = TempDir::new("roundtrip");
        let mut p = LanguageProgress::new();
        p.record_stage_completion(1, "2026-08-21");
        p.record_stage_completion(2, "2026-08-22");
        save(root.path(), &lang("french"), &p).unwrap();

        let loaded = load(root.path(), &lang("french")).unwrap();
        assert_eq!(loaded.completed_stages(), vec![1, 2]);
        assert_eq!(loaded.streak_on("2026-08-22"), 2);
    }

    #[test]
    fn missing_file_is_fresh_progress() {
        let root = TempDir::new("missing");
        assert_eq!(
            load(root.path(), &lang("klingon"))
                .unwrap()
                .completed_stages(),
            Vec::<u32>::new()
        );
    }

    #[test]
    fn unknown_line_is_an_error() {
        let root = TempDir::new("strict");
        fs::create_dir_all(root.path().join("progress")).unwrap();
        fs::write(
            root.path().join("progress/french.txt"),
            "stage 1\nfrobnicate 7\n",
        )
        .unwrap();
        assert!(matches!(
            load(root.path(), &lang("french")),
            Err(StoreError::Parse(_))
        ));
    }

    #[test]
    fn bad_day_lines_are_errors() {
        for (name, content) in [
            ("garbage", "stage 1\nday yesterday\n"),
            ("notreal", "day 2026-02-30\n"),
            ("badmonth", "day 2026-13-01\n"),
            ("shortform", "day 2026-9-9\n"),
        ] {
            let root = TempDir::new(&format!("day-strict-{name}"));
            fs::create_dir_all(root.path().join("progress")).unwrap();
            fs::write(root.path().join("progress/french.txt"), content).unwrap();
            assert!(
                matches!(
                    load(root.path(), &lang("french")),
                    Err(StoreError::Parse(_))
                ),
                "{name} should be a parse error"
            );
        }
    }

    #[test]
    fn srs_round_trips_through_disk() {
        let root = TempDir::new("srs-roundtrip");
        let mut s = crate::srs::SrsState::new();
        // one vocabulary entry that contains spaces, to prove the
        // split-on-markers parse keeps such words intact
        s.introduce(
            &["bonjour".to_string(), "c'est a dire".to_string()],
            "2026-10-30",
        );
        s.answer("bonjour", true, "2026-10-30");
        save_srs(root.path(), &lang("french"), &s).unwrap();

        let loaded = load_srs(root.path(), &lang("french")).unwrap();
        let (_, bonjour) = loaded
            .cards()
            .into_iter()
            .find(|(w, _)| *w == "bonjour")
            .unwrap();
        assert_eq!(bonjour.box_number, 2);
        assert_eq!(bonjour.due, "2026-11-01");
        // the untouched card is still due today
        assert_eq!(loaded.due_on("2026-10-30").len(), 1);
    }

    #[test]
    fn missing_srs_file_is_a_fresh_schedule() {
        let root = TempDir::new("srs-missing");
        assert_eq!(
            load_srs(root.path(), &lang("klingon"))
                .unwrap()
                .cards()
                .len(),
            0
        );
    }

    #[test]
    fn bad_srs_lines_are_errors() {
        for (name, content) in [
            ("unknown", "stage 1\n"),
            ("nobox", "word bonjour due 2026-10-30\n"),
            ("nodue", "word bonjour box 2\n"),
            ("badbox", "word bonjour box x due 2026-10-30\n"),
            ("rangebox", "word bonjour box 9 due 2026-10-30\n"),
            ("zerobox", "word bonjour box 0 due 2026-10-30\n"),
            ("baddate", "word bonjour box 2 due 2026-13-01\n"),
            ("emptyword", "word  box 2 due 2026-10-30\n"),
        ] {
            let root = TempDir::new(&format!("srs-strict-{name}"));
            fs::create_dir_all(root.path().join("srs")).unwrap();
            fs::write(root.path().join("srs/french.txt"), content).unwrap();
            assert!(
                matches!(
                    load_srs(root.path(), &lang("french")),
                    Err(StoreError::Parse(_))
                ),
                "{name} should be a parse error"
            );
        }
    }
}
