//! SRS — spaced-repetition scheduling, domain types only.
//!
//! This module sits beside `engine/` and `progress/` as pure domain
//! logic: it knows nothing about files, paths, or any storage format.
//! `storage/` is the only module allowed to know about `srs::` types
//! AND the on-disk plain-text format at the same time; it converts
//! between the two. `session/` drives the scheduling rules through the
//! API below, and `cli/` decides which words qualify (vocabulary of
//! completed stages) and what "today" is.
//!
//! The scheduler is classic Leitner boxes: six boxes with review
//! intervals of 1/2/4/8/16/32 days. A correct answer promotes a word
//! one box (capped at box 6), a wrong answer demotes it to box 1, and
//! the next review date is always `today + interval(new box)` —
//! deterministic, readable in a plain-text file, and easy to reason
//! about when it changes.

use std::collections::BTreeMap;

/// Review interval per box, in days: box 1 → 1 day, … box 6 → 32 days.
pub const BOX_INTERVALS_DAYS: [u32; 6] = [1, 2, 4, 8, 16, 32];

/// The highest Leitner box. A word that keeps being answered correctly
/// stays here and reviews every 32 days.
pub const MAX_BOX: u8 = 6;

/// One scheduled word: which box it sits in and when it is due next.
/// The word itself is the key in `SrsState`, not a field here, so a
/// card and its key can never disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub box_number: u8,
    /// Next due date, YYYY-MM-DD.
    pub due: String,
}

/// Where a word lands after one review — `answer` returns this so the
/// caller can tell the learner what just happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub box_number: u8,
    pub due: String,
}

/// All scheduled words for one language, keyed by word. A `BTreeMap`
/// keeps the word order stable, so serialized state files are
/// diff-friendly and review sessions run in a deterministic order.
#[derive(Debug, Default, Clone)]
pub struct SrsState {
    cards: BTreeMap<String, Card>,
}

impl SrsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds state from parts. This is the `storage/` entry point:
    /// the plain-text layer parses its file into (word, box, due)
    /// triples and hands them here so the invariants stay in one place.
    /// Duplicate words keep the last entry, matching a line-by-line
    /// read of the file.
    pub fn from_parts(entries: Vec<(String, u8, String)>) -> Self {
        Self {
            cards: entries
                .into_iter()
                .map(|(word, box_number, due)| (word, Card { box_number, due }))
                .collect(),
        }
    }

    /// All cards with their words, sorted by word — the shape `storage/`
    /// serializes.
    pub fn cards(&self) -> Vec<(&str, &Card)> {
        self.cards.iter().map(|(w, c)| (w.as_str(), c)).collect()
    }

    /// Schedules every not-yet-known word at box 1, due on `date`, so a
    /// freshly completed stage's vocabulary is reviewable right away.
    /// Words already scheduled keep their box and due date — being
    /// re-introduced never resets progress. Returns how many words were
    /// newly scheduled.
    pub fn introduce(&mut self, words: &[String], date: &str) -> usize {
        let before = self.cards.len();
        for word in words {
            self.cards.entry(word.clone()).or_insert_with(|| Card {
                box_number: 1,
                due: date.to_string(),
            });
        }
        self.cards.len() - before
    }

    /// Cards due on or before `date`: everything overdue plus what is
    /// due today. Word order is alphabetical, which makes review
    /// sessions deterministic.
    pub fn due_on(&self, date: &str) -> Vec<(&str, &Card)> {
        self.cards
            .iter()
            .filter(|(_, c)| c.due.as_str() <= date)
            .map(|(w, c)| (w.as_str(), c))
            .collect()
    }

    /// Applies one review result. Correct promotes one box (capped at
    /// `MAX_BOX`), wrong demotes to box 1; the next due date is
    /// `date + interval` of the NEW box. Returns the new schedule, or
    /// None when the word has no card (callers `introduce` first) or
    /// `date` is not a real calendar date — in which case the card is
    /// left untouched.
    pub fn answer(&mut self, word: &str, correct: bool, date: &str) -> Option<Schedule> {
        let card = self.cards.get(word)?;
        let box_number = if correct {
            (card.box_number + 1).min(MAX_BOX)
        } else {
            1
        };
        let due = add_days(date, i64::from(interval_days(box_number)))?;
        let card = self.cards.get_mut(word)?;
        card.box_number = box_number;
        card.due = due.clone();
        Some(Schedule { box_number, due })
    }

    /// The earliest due date after `date` and how many cards fall on
    /// it, for the "nothing due today" message. None when nothing is
    /// scheduled beyond `date`.
    pub fn next_due(&self, date: &str) -> Option<(&str, usize)> {
        let mut upcoming: Vec<&str> = self
            .cards
            .values()
            .map(|c| c.due.as_str())
            .filter(|d| *d > date)
            .collect();
        upcoming.sort_unstable();
        let first = *upcoming.first()?;
        Some((first, upcoming.iter().filter(|d| **d == first).count()))
    }
}

/// Review interval for a box number, in days. Out-of-range boxes (which
/// strict parsing keeps out of the state anyway) clamp to the ends.
fn interval_days(box_number: u8) -> u32 {
    BOX_INTERVALS_DAYS[(box_number.clamp(1, MAX_BOX) - 1) as usize]
}

/// Adds `n` days to a YYYY-MM-DD date, carrying across month and year
/// boundaries — and backwards when `n` is negative, which is how the
/// CLI's look-back windows (last N days) are computed without a second
/// function. None when `date` is malformed or not a real calendar date
/// ("2026-02-30" and "2026-13-01" are rejected). Same no-time-crate
/// stance as `progress/`: the math is small and owned (§6).
pub fn add_days(date: &str, n: i64) -> Option<String> {
    let (y, m, d) = parse_date(date)?;
    let days = days_from_civil(y, m, d) + n;
    let (y, m, d) = civil_from_days(days);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// Whether `s` is a real YYYY-MM-DD calendar date. `storage/` uses this
/// to keep unparseable due dates out of the state.
pub fn valid_date(s: &str) -> bool {
    parse_date(s).is_some()
}

/// Parses and validates a civil date, month and day ranges included.
fn parse_date(date: &str) -> Option<(i64, i64, i64)> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i64 = date.get(..4)?.parse().ok()?;
    let m: i64 = date.get(5..7)?.parse().ok()?;
    let d: i64 = date.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || d < 1 || d > month_len(y, m) {
        return None;
    }
    Some((y, m, d))
}

fn month_len(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            if leap {
                29
            } else {
                28
            }
        }
    }
}

/// Days since the unix epoch for a civil date — Howard Hinnant's
/// public-domain algorithm, the same one `cli/` uses for "today".
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date from days since the unix epoch — the inverse of the above.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn introduce_schedules_new_words_due_same_day() {
        let mut s = SrsState::new();
        let words = vec!["bonjour".to_string(), "merci".to_string()];
        assert_eq!(s.introduce(&words, "2026-10-30"), 2);
        assert_eq!(s.due_on("2026-10-30").len(), 2);
        // a second run changes nothing — scheduled words keep their state
        assert_eq!(s.introduce(&words, "2026-12-25"), 0);
        assert_eq!(s.due_on("2026-10-30").len(), 2);
    }

    #[test]
    fn correct_answers_climb_the_interval_ladder() {
        let mut s = SrsState::new();
        s.introduce(&["bonjour".to_string()], "2026-10-30");
        // box 1 → 2: due in 2 days
        let sched = s.answer("bonjour", true, "2026-10-30").unwrap();
        assert_eq!(
            sched,
            Schedule {
                box_number: 2,
                due: "2026-11-01".into()
            }
        );
        // box 2 → 3: due 4 days later
        let sched = s.answer("bonjour", true, "2026-11-01").unwrap();
        assert_eq!(
            sched,
            Schedule {
                box_number: 3,
                due: "2026-11-05".into()
            }
        );
    }

    #[test]
    fn wrong_answer_resets_to_box_one_due_tomorrow() {
        let mut s = SrsState::new();
        s.introduce(&["merci".to_string()], "2026-10-30");
        s.answer("merci", true, "2026-10-30");
        s.answer("merci", true, "2026-11-01");
        let sched = s.answer("merci", false, "2026-11-05").unwrap();
        assert_eq!(
            sched,
            Schedule {
                box_number: 1,
                due: "2026-11-06".into()
            }
        );
    }

    #[test]
    fn box_six_is_the_ceiling() {
        let mut s = SrsState::from_parts(vec![("mot".to_string(), 6, "2026-10-30".to_string())]);
        let sched = s.answer("mot", true, "2026-10-30").unwrap();
        assert_eq!(
            sched,
            Schedule {
                box_number: 6,
                due: "2026-12-01".into()
            }
        );
    }

    #[test]
    fn due_on_includes_overdue_and_excludes_future() {
        let s = SrsState::from_parts(vec![
            ("aaa".to_string(), 2, "2026-10-28".to_string()),
            ("bbb".to_string(), 2, "2026-10-30".to_string()),
            ("ccc".to_string(), 2, "2026-11-02".to_string()),
        ]);
        let due: Vec<&str> = s.due_on("2026-10-30").into_iter().map(|(w, _)| w).collect();
        assert_eq!(due, vec!["aaa", "bbb"]);
    }

    #[test]
    fn next_due_reports_earliest_future_date() {
        let s = SrsState::from_parts(vec![
            ("aaa".to_string(), 2, "2026-11-03".to_string()),
            ("bbb".to_string(), 2, "2026-11-02".to_string()),
            ("ccc".to_string(), 2, "2026-11-02".to_string()),
            ("ddd".to_string(), 2, "2026-10-30".to_string()),
        ]);
        assert_eq!(s.next_due("2026-10-30"), Some(("2026-11-02", 2usize)));
        assert_eq!(SrsState::new().next_due("2026-10-30"), None);
    }

    #[test]
    fn answer_unknown_word_or_bad_date_returns_none() {
        let mut s = SrsState::new();
        s.introduce(&["bonjour".to_string()], "2026-10-30");
        assert!(s.answer("inconnu", true, "2026-10-30").is_none());
        assert!(s.answer("bonjour", true, "not-a-date").is_none());
        // a bad date leaves the card untouched
        let (_, card) = s
            .cards()
            .into_iter()
            .find(|(w, _)| *w == "bonjour")
            .unwrap();
        assert_eq!(card.box_number, 1);
        assert_eq!(card.due, "2026-10-30");
    }

    #[test]
    fn from_parts_keeps_the_last_duplicate() {
        let s = SrsState::from_parts(vec![
            ("mot".to_string(), 2, "2026-11-01".to_string()),
            ("mot".to_string(), 4, "2026-12-01".to_string()),
        ]);
        assert_eq!(s.cards().len(), 1);
        assert_eq!(s.cards()[0].1.box_number, 4);
    }

    #[test]
    fn add_days_crosses_month_year_and_leap_boundaries() {
        assert_eq!(add_days("2026-10-30", 0).as_deref(), Some("2026-10-30"));
        assert_eq!(add_days("2026-10-30", 1).as_deref(), Some("2026-10-31"));
        assert_eq!(add_days("2026-10-31", 1).as_deref(), Some("2026-11-01"));
        assert_eq!(add_days("2026-12-31", 1).as_deref(), Some("2027-01-01"));
        assert_eq!(add_days("2024-02-28", 1).as_deref(), Some("2024-02-29"));
        assert_eq!(add_days("2026-02-28", 1).as_deref(), Some("2026-03-01"));
        assert_eq!(add_days("2026-01-31", 32).as_deref(), Some("2026-03-04"));
        assert_eq!(add_days("garbage", 1), None);
        assert_eq!(add_days("2026-02-30", 1), None);
        assert_eq!(add_days("2026-13-01", 1), None);
    }

    #[test]
    fn add_days_walks_backwards_for_look_back_windows() {
        // negative n crosses the same boundaries in reverse
        assert_eq!(add_days("2026-10-30", -1).as_deref(), Some("2026-10-29"));
        assert_eq!(add_days("2026-11-01", -1).as_deref(), Some("2026-10-31"));
        assert_eq!(add_days("2026-03-01", -1).as_deref(), Some("2026-02-28"));
        assert_eq!(add_days("2024-03-01", -1).as_deref(), Some("2024-02-29"));
        assert_eq!(add_days("2026-01-01", -1).as_deref(), Some("2025-12-31"));
        assert_eq!(add_days("2026-10-30", -13).as_deref(), Some("2026-10-17"));
    }

    #[test]
    fn valid_date_accepts_only_real_dates() {
        assert!(valid_date("2026-10-30"));
        assert!(!valid_date("2026-10-32"));
        assert!(!valid_date("2026-13-01"));
        assert!(!valid_date("2026-9-30"));
        assert!(!valid_date("2026-10-3"));
        assert!(!valid_date(""));
    }
}
