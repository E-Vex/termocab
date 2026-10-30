//! Progress Tracker — domain types only.
//!
//! This module mirrors `engine/`: it holds the in-memory shape of a
//! learner's progress and the rules computed from it, and knows nothing
//! about files, paths, or any storage format. `storage/` is the only
//! module allowed to know about `progress::` types AND about the on-disk
//! plain-text format at the same time; it converts between the two.

use std::collections::HashSet;

/// One language's progress. Stage numbers are the only level concept the
/// user ever sees (no CEFR anywhere near here).
#[derive(Debug, Default, Clone)]
pub struct LanguageProgress {
    /// Every stage id completed (passed with >= 80% correct in one run).
    completed: HashSet<u32>,
    /// Every calendar date (YYYY-MM-DD, local time) on which the learner
    /// completed at least one stage.
    active_days: Vec<String>,
}

impl LanguageProgress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a calendar date as active. Recording the day is its own
    /// operation (not just a side effect of completing a stage) because
    /// review sessions keep a streak alive the same way — the caller
    /// decides what counts as activity. Idempotent per date.
    pub fn record_active_day(&mut self, date: &str) {
        if !self.active_days.contains(&date.to_string()) {
            self.active_days.push(date.to_string());
        }
    }

    /// Records a stage as completed, plus the date it was completed on.
    /// The date is an opaque YYYY-MM-DD string — deciding what "today" is
    /// belongs to the caller (the app layer), not to this module.
    pub fn record_stage_completion(&mut self, stage_id: u32, date: &str) {
        self.completed.insert(stage_id);
        self.record_active_day(date);
    }

    pub fn is_stage_completed(&self, stage_id: u32) -> bool {
        self.completed.contains(&stage_id)
    }

    /// The next uncompleted stage, or None when every known stage is done.
    /// Callers pair this with the loaded stage list: it answers "what
    /// should I play next?", not "what stages exist?".
    pub fn next_stage<'a>(
        &self,
        stages: &'a [crate::engine::Stage],
    ) -> Option<&'a crate::engine::Stage> {
        stages
            .iter()
            .find(|s| !self.completed.contains(&s.id))
    }

    /// The stage the learner should play: the first stage not yet completed.
    pub fn current_stage_id(&self, stages: &[crate::engine::Stage]) -> Option<u32> {
        self.next_stage(stages).map(|s| s.id)
    }

    /// A streak is consecutive active days ending today or yesterday — so
    /// the streak survives if you miss "today" while it's still going,
    /// and survives entirely if you're active today. Computed from
    /// `active_days`, never stored, so it can't drift out of sync.
    pub fn streak_on(&self, today: &str) -> u32 {
        if self.active_days.is_empty() {
            return 0;
        }
        let mut sorted: Vec<&str> = self.active_days.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.dedup();

        // If neither today nor yesterday is active, the streak is dead.
        let last = *sorted.last().expect("checked non-empty");
        let yesterday = match prev_day(today) {
            Some(p) => p,
            None => return 0, // caller gave a malformed date
        };
        if last != today && last != yesterday {
            return 0;
        }

        // Walk backwards from the last active day, counting consecutive days.
        let mut streak = 1u32;
        let mut cursor = last.to_string();
        while let Some(prev) = prev_day(&cursor) {
            if sorted.binary_search(&prev.as_str()).is_ok() {
                streak += 1;
                cursor = prev;
            } else {
                break;
            }
        }
        streak
    }

    /// All completed stage ids, sorted, for display.
    pub fn completed_stages(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.completed.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// All distinct active dates, sorted — exposed for `storage/`, which
    /// needs the full history to serialize.
    pub fn active_days(&self) -> &[String] {
        &self.active_days
    }

    /// Rebuilds progress from parts. This is the `storage/` entry point:
    /// the plain-text layer parses its file into completed ids and dates,
    /// then hands them here so the invariants stay in one place.
    pub fn from_parts(completed: Vec<u32>, active_days: Vec<String>) -> Self {
        Self {
            completed: completed.into_iter().collect(),
            active_days,
        }
    }
}

/// Previous day of a YYYY-MM-DD string, without any time crate: the
/// calendar math is small, and pulling in chrono for one subtraction is
/// exactly the reflexive dependency the handoff warns against (§6).
fn prev_day(date: &str) -> Option<String> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i64 = date.get(..4)?.parse().ok()?;
    let m: i64 = date.get(5..7)?.parse().ok()?;
    let d: i64 = date.get(8..10)?.parse().ok()?;

    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_len = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if !(1..=12).contains(&m) || d < 1 || d > month_len {
        return None;
    }

    let (ny, nm, nd) = if d > 1 {
        (y, m, d - 1)
    } else {
        match m {
            1 => (y - 1, 12, 31),
            _ => {
                let prev_len = match m {
                    3 => {
                        if leap { 29 } else { 28 }
                    }
                    5 | 7 | 10 | 12 => 30,
                    _ => 31,
                };
                (y, m - 1, prev_len)
            }
        }
    };
    Some(format!("{:04}-{:02}-{:02}", ny, nm, nd))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(id: u32) -> crate::engine::Stage {
        crate::engine::Stage {
            id,
            title: format!("Stage {id}"),
            level_hint: "beginner".into(),
            vocabulary: vec![],
            exercises: vec![],
        }
    }

    #[test]
    fn completion_is_idempotent_per_date() {
        let mut p = LanguageProgress::new();
        p.record_stage_completion(1, "2026-08-22");
        p.record_stage_completion(1, "2026-08-22");
        assert_eq!(p.completed_stages(), vec![1]);
        assert_eq!(p.active_days(), &["2026-08-22".to_string()]);
    }

    #[test]
    fn next_stage_skips_completed() {
        let stages = [stage(1), stage(2), stage(3)];
        let mut p = LanguageProgress::new();
        p.record_stage_completion(1, "2026-08-22");
        assert_eq!(p.next_stage(&stages).unwrap().id, 2);
        assert_eq!(p.current_stage_id(&stages), Some(2));
    }

    #[test]
    fn streak_counts_consecutive_days() {
        let mut p = LanguageProgress::new();
        for (id, d) in [(1, "2026-08-20"), (2, "2026-08-21"), (3, "2026-08-22")] {
            p.record_stage_completion(id, d);
        }
        assert_eq!(p.streak_on("2026-08-22"), 3);
        // yesterday's streak still counts if you haven't played today
        assert_eq!(p.streak_on("2026-08-23"), 3);
        // but skipping one full day kills it
        assert_eq!(p.streak_on("2026-08-24"), 0);
    }

    #[test]
    fn streak_zero_when_inactive() {
        let p = LanguageProgress::new();
        assert_eq!(p.streak_on("2026-08-22"), 0);
    }

    #[test]
    fn prev_day_handles_month_and_year_edges() {
        assert_eq!(prev_day("2026-03-01").as_deref(), Some("2026-02-28"));
        assert_eq!(prev_day("2024-03-01").as_deref(), Some("2024-02-29"));
        assert_eq!(prev_day("2026-01-01").as_deref(), Some("2025-12-31"));
        assert_eq!(prev_day("2026-12-31").as_deref(), Some("2026-12-30"));
        assert_eq!(prev_day("garbage"), None);
        assert_eq!(prev_day("2026-13-01"), None);
    }

    #[test]
    fn active_day_recording_is_idempotent() {
        let mut p = LanguageProgress::new();
        p.record_active_day("2026-10-30");
        p.record_active_day("2026-10-30");
        assert_eq!(p.active_days(), &["2026-10-30".to_string()]);
        assert_eq!(p.streak_on("2026-10-30"), 1);
    }

    #[test]
    fn from_parts_round_trips() {
        let p = LanguageProgress::from_parts(vec![1, 2], vec!["2026-08-22".into()]);
        assert!(p.is_stage_completed(2));
        assert_eq!(p.streak_on("2026-08-22"), 1);
    }
}
