//! Session runner — the application layer between the CLI and the
//! engine. Runs one stage's exercises in the terminal, grades them
//! through `engine::`, applies the >= 80% completion rule, and records
//! the outcome via `progress::`. Runs spaced-repetition review sessions
//! the same way: due cards in, rescheduled cards out, grading through
//! `engine::`. It talks to the user through the `Terminal` trait so
//! unit tests can drive it with a scripted player instead of a real
//! stdin/stdout.

use crate::engine::{Exercise, Grade, Stage};
use crate::progress::LanguageProgress;
use std::io::{self, BufRead, Write};

/// How a stage run ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Enough exercises were answered correctly — stage completed.
    Passed { correct: usize, total: usize },
    /// Below the pass threshold — try again.
    Failed { correct: usize, total: usize },
}

impl Outcome {
    pub fn passed(&self) -> bool {
        matches!(self, Outcome::Passed { .. })
    }
}

/// How a review session went — enough for the caller to decide whether
/// the day counts as active (any card answered) and what to save.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReviewOutcome {
    pub answered: usize,
    pub correct: usize,
}

/// Exercise order for a stage run (decision D7).
///
/// Learners memorize positions, not words, when every run presents the
/// same sequence — so stages shuffle by default. A named seed makes
/// the order reproducible (`--seed N`, for tests and for comparing two
/// runs); `InOrder` is the authored content order (`--no-shuffle`).
/// Reviews are deliberately not shuffled: their order follows due
/// dates and stays deterministic (§7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseOrder {
    /// Content order, exactly as authored.
    InOrder,
    /// Fisher–Yates under this seed — same seed, same order.
    Shuffled(u64),
}

/// How a session presents a stage: the exercise order. Resolved at the
/// CLI edge (flags, OS seed) and injected here, so the runner stays
/// pure and testable against a fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    pub order: ExerciseOrder,
}

impl Default for RunOptions {
    /// The shape tests script against: authored order.
    fn default() -> Self {
        Self {
            order: ExerciseOrder::InOrder,
        }
    }
}

/// Shuffles `items` with Fisher–Yates, driven by a SplitMix64
/// generator — 11 lines of std-only code, which is the whole reason
/// it exists instead of a `rand` dependency (§6). Deterministic per
/// seed, which is what `--seed N` promises. The modulo bias for n
/// items is n/2^64 — beyond irrelevant at vocabulary sizes.
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed;
    for i in (1..items.len()).rev() {
        let j = (splitmix64(&mut state) % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// SplitMix64: one step, one well-spread value.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The order exercises are presented in for a run: either the
/// authored sequence or its seeded permutation.
fn exercise_sequence(total: usize, order: ExerciseOrder) -> Vec<usize> {
    let mut sequence: Vec<usize> = (0..total).collect();
    if let ExerciseOrder::Shuffled(seed) = order {
        shuffle(&mut sequence, seed);
    }
    sequence
}

/// One line of user input, or why there is not one. EOF (the learner
/// pressed Ctrl-D) and unreadable input (invalid UTF-8, an I/O error)
/// are distinct on purpose: the runner reports them differently instead
/// of describing both as "input closed" (review item 11).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Input {
    /// A line as typed, trailing newline included.
    Line(String),
    /// End of input.
    Closed,
    /// Input arrived but could not be read; carries the underlying
    /// error text ("stream did not contain valid UTF-8", …).
    Unreadable(String),
}

/// Minimal terminal the runner needs. Tests implement it with a queue of
/// pre-scripted answers; real usage uses `StdioTerminal`.
///
/// Printing can fail — a closed stdout (`termocab start french | head`)
/// is a broken pipe — so `print` returns the error instead of letting
/// `println!` panic mid-session (decision D6). The runner stops on the
/// first failure; the caller treats the run as aborted and saves
/// nothing.
pub trait Terminal {
    fn print(&mut self, line: &str) -> io::Result<()>;
    /// Reads one line of user input.
    fn read_line(&mut self) -> Input;
}

/// The real terminal: stdout + stdin.
pub struct StdioTerminal;

impl Terminal for StdioTerminal {
    fn print(&mut self, line: &str) -> io::Result<()> {
        let mut out = io::stdout().lock();
        writeln!(out, "{line}")?;
        out.flush()
    }

    fn read_line(&mut self) -> Input {
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) => Input::Closed, // EOF
            Ok(_) => Input::Line(line),
            Err(e) => Input::Unreadable(e.to_string()), // e.g. invalid UTF-8
        }
    }
}

/// Display form of one exercise: prompt plus, for multiple choice, the
/// lettered options — git-style plain text, no menus, no cursor tricks.
fn render(stage_title: &str, index: usize, total: usize, exercise: &Exercise) -> String {
    let mut out = format!("[{stage_title} — {index}/{total}] {}", exercise.prompt());
    if let Exercise::MultipleChoice { options, .. } = exercise {
        out.push('\n');
        for (i, opt) in options.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", option_letter(i), opt));
        }
        out.push_str("(answer with a letter or the word itself)");
    }
    out
}

/// The option letter for a 0-based index: 'a' for the first option,
/// 'b' for the second, and so on. Content validation caps options at
/// 26, so every index has a letter — the assert documents that
/// invariant instead of silently wrapping past 'z' the way
/// `b'a' + i as u8` does.
fn option_letter(index: usize) -> char {
    assert!(index < 26, "content validation caps options at 26");
    (b'a' + index as u8) as char
}

/// Resolves a multiple-choice attempt: 'b' picks the second option; any
/// other text is compared against the answer directly.
fn resolve_attempt(exercise: &Exercise, raw: &str) -> String {
    if let Exercise::MultipleChoice { options, .. } = exercise {
        let trimmed = raw.trim().to_lowercase();
        let mut chars = trimmed.chars();
        // exactly one character, and that character is an option letter
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if let Some(index) = letter_index(c) {
                if index < options.len() {
                    return options[index].clone();
                }
            }
        }
    }
    raw.to_string()
}

/// 'a' -> 0, 'b' -> 1, … 'z' -> 25; anything else (digits, accents,
/// multi-byte characters) is not an option letter.
fn letter_index(c: char) -> Option<usize> {
    if c.is_ascii_lowercase() {
        Some((c as u8 - b'a') as usize)
    } else {
        None
    }
}

/// Runs every exercise in the stage once — in [`RunOptions`] order —
/// then reports the outcome. Pure with respect to storage: the caller
/// decides what to do with the result. Fails when the terminal goes
/// away mid-run (a closed stdout is a broken pipe, D6) — an aborted
/// run reports no outcome.
pub fn run_stage<T: Terminal>(
    term: &mut T,
    stage: &Stage,
    opts: &RunOptions,
) -> Result<Outcome, io::Error> {
    let total = stage.exercises.len();
    let sequence = exercise_sequence(total, opts.order);
    let mut correct = 0usize;
    term.print(&format!("— Stage {}: {} —", stage.id, stage.title))?;
    term.print(&format!(
        "{total} exercise(s). Answer each, then press Enter."
    ))?;

    for (i, &n) in sequence.iter().enumerate() {
        let exercise = &stage.exercises[n];
        term.print("")?;
        term.print(&render(&stage.title, i + 1, total, exercise))?;
        let answer = match term.read_line() {
            Input::Line(line) => resolve_attempt(exercise, &line),
            Input::Closed => {
                term.print("input closed — stopping this stage.")?;
                break;
            }
            Input::Unreadable(e) => {
                term.print(&format!(
                    "could not read input ({e}) — stopping this stage."
                ))?;
                break;
            }
        };
        if exercise.is_correct(&answer) {
            correct += 1;
            match exercise.grade(&answer) {
                Grade::Correct => term.print("correct.")?,
                Grade::CorrectWithHint(expected) => {
                    term.print(&format!("correct — watch the accents: {expected}"))?
                }
                Grade::Incorrect => unreachable!("is_correct said otherwise"),
            }
        } else {
            term.print(&format!("not quite — expected: {}", exercise.answer()))?;
        }
    }

    let outcome = if stage.is_completed_by(correct) {
        Outcome::Passed { correct, total }
    } else {
        Outcome::Failed { correct, total }
    };
    term.print("")?;
    match outcome {
        Outcome::Passed { correct, total } => {
            term.print(&format!("stage complete: {correct}/{total} correct. nice."))?
        }
        Outcome::Failed { correct, total } => term.print(&format!(
            "{correct}/{total} correct — below the 80% bar. run it again when ready."
        ))?,
    }
    Ok(outcome)
}

/// Convenience wrapper: run the stage, and if it passed, record it in
/// progress with the given date (the caller decides what "today" is).
/// A run that dies on a closed stdout records nothing — an aborted run
/// is not a completed stage (D6).
pub fn run_and_record<T: Terminal>(
    term: &mut T,
    stage: &Stage,
    progress: &mut LanguageProgress,
    today: &str,
    opts: &RunOptions,
) -> Result<Outcome, io::Error> {
    let outcome = run_stage(term, stage, opts)?;
    if let Outcome::Passed { .. } = outcome {
        progress.record_stage_completion(stage.id, today);
    }
    Ok(outcome)
}

/// Runs one spaced-repetition review: every card due on `today` is
/// shown as a plain translation prompt — the foreign word, the learner
/// types its translation — graded with the same case-insensitive rule
/// as stage exercises, and rescheduled through `srs::`. Pure with
/// respect to storage: the caller decides what to do with the outcome.
///
/// A card whose word is missing from `vocabulary` (the content changed
/// under it) is skipped and left untouched, rather than crashing or
/// silently resetting it.
pub fn run_review<T: Terminal>(
    term: &mut T,
    srs: &mut crate::srs::SrsState,
    vocabulary: &[&crate::engine::Vocabulary],
    today: &str,
) -> Result<ReviewOutcome, io::Error> {
    let due: Vec<String> = srs
        .due_on(today)
        .into_iter()
        .map(|(w, _)| w.to_string())
        .collect();
    let total = due.len();
    let mut outcome = ReviewOutcome::default();

    term.print(&format!("— review: {total} word(s) —"))?;
    term.print("give the translation of each word, then press Enter.")?;

    for (i, word) in due.iter().enumerate() {
        let Some(v) = vocabulary.iter().find(|v| v.word == *word) else {
            term.print(&format!(
                "skipping '{word}' — no longer in the loaded content."
            ))?;
            continue;
        };
        term.print("")?;
        term.print(&format!("[{}/{}] {}", i + 1, total, v.word))?;
        let attempt = match term.read_line() {
            Input::Line(line) => line,
            Input::Closed => {
                term.print("input closed — stopping this review.")?;
                break;
            }
            Input::Unreadable(e) => {
                term.print(&format!(
                    "could not read input ({e}) — stopping this review."
                ))?;
                break;
            }
        };
        // a review card IS a translation exercise: this reuses the
        // engine's grading (the full normalization table, accent hint
        // included) instead of growing a second, slightly different
        // rule here. The vocabulary's accepted_translations ride along
        // as accepted_answers, so "hello" counts for a word whose
        // primary translation is "hi" (D8)
        let exercise = Exercise::Translation {
            prompt: v.word.clone(),
            answer: v.translation.clone(),
            accepted_answers: v.accepted_translations.clone(),
        };
        outcome.answered += 1;
        let grade = exercise.grade(&attempt);
        let right = !matches!(grade, Grade::Incorrect);
        if right {
            outcome.correct += 1;
        }
        let hint = match &grade {
            Grade::CorrectWithHint(expected) => format!(" (watch the accents: {expected})"),
            _ => String::new(),
        };
        match srs.answer(word, right, today) {
            Some(s) if right => term.print(&format!(
                "correct{hint} — box {}, due {}.",
                s.box_number, s.due
            ))?,
            Some(s) => term.print(&format!(
                "not quite — expected: {}. back to box {}, due {}.",
                v.translation, s.box_number, s.due
            ))?,
            None if right => term.print(&format!("correct{hint}."))?,
            None => term.print(&format!("not quite — expected: {}.", v.translation))?,
        }
    }

    term.print("")?;
    term.print(&format!(
        "reviewed {} word(s): {} correct, {} to revisit.",
        outcome.answered,
        outcome.correct,
        outcome.answered - outcome.correct
    ))?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player with scripted input, capturing everything printed.
    struct Scripted {
        inputs: Vec<Input>,
        printed: Vec<String>,
        at: usize,
    }

    impl Scripted {
        fn new(answers: &[&str]) -> Self {
            Self::with_inputs(
                answers
                    .iter()
                    .map(|a| Input::Line((*a).to_string()))
                    .collect(),
            )
        }

        fn with_inputs(inputs: Vec<Input>) -> Self {
            Self {
                inputs,
                printed: vec![],
                at: 0,
            }
        }
    }

    impl Terminal for Scripted {
        fn print(&mut self, line: &str) -> io::Result<()> {
            self.printed.push(line.to_string());
            Ok(())
        }
        fn read_line(&mut self) -> Input {
            let input = self.inputs.get(self.at).cloned();
            self.at += 1;
            input.unwrap_or(Input::Closed)
        }
    }

    /// A terminal whose stdout goes away after `prints_left` lines —
    /// what `termocab start french | head` looks like from the inside
    /// (D6).
    struct ClosedPipe {
        prints_left: usize,
    }

    impl Terminal for ClosedPipe {
        fn print(&mut self, _line: &str) -> io::Result<()> {
            if self.prints_left == 0 {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.prints_left -= 1;
            Ok(())
        }
        fn read_line(&mut self) -> Input {
            Input::Line("whatever\n".into())
        }
    }

    fn fixture_stage() -> Stage {
        Stage {
            id: 1,
            title: "First Steps".into(),
            level_hint: "beginner".into(),
            vocabulary: vec![],
            exercises: vec![
                Exercise::Translation {
                    prompt: "Je suis etudiant.".into(),
                    answer: "I am a student".into(),
                    accepted_answers: vec![],
                },
                Exercise::MultipleChoice {
                    prompt: "Choose the correct greeting:".into(),
                    options: vec!["maison".into(), "bonjour".into(), "manger".into()],
                    answer: "bonjour".into(),
                },
                Exercise::FillBlank {
                    prompt: "___, comment ca va ?".into(),
                    answer: "Bonjour".into(),
                    accepted_answers: vec![],
                },
                Exercise::FillBlank {
                    prompt: "Je ___ etudiant.".into(),
                    answer: "suis".into(),
                    accepted_answers: vec![],
                },
            ],
        }
    }

    #[test]
    fn all_correct_passes_and_records() {
        let mut p = LanguageProgress::new();
        let mut t = Scripted::new(&["I am a student", "b", "bonjour", "suis"]);
        let out = run_and_record(
            &mut t,
            &fixture_stage(),
            &mut p,
            "2026-08-22",
            &RunOptions::default(),
        )
        .unwrap();
        assert_eq!(
            out,
            Outcome::Passed {
                correct: 4,
                total: 4
            }
        );
        assert!(p.is_stage_completed(1));
        assert_eq!(p.active_days(), &["2026-08-22".to_string()]);
    }

    #[test]
    fn grading_is_case_insensitive() {
        let mut t = Scripted::new(&["i AM a student", "b", "  bonjour  ", "SUIS"]);
        let out = run_stage(&mut t, &fixture_stage(), &RunOptions::default()).unwrap();
        assert_eq!(
            out,
            Outcome::Passed {
                correct: 4,
                total: 4
            }
        );
    }

    #[test]
    fn three_of_four_is_below_the_bar() {
        // 3/4 = 75% — just under the 80% bar.
        let mut t = Scripted::new(&["wrong", "b", "bonjour", "suis"]);
        let out = run_stage(&mut t, &fixture_stage(), &RunOptions::default()).unwrap();
        assert_eq!(
            out,
            Outcome::Failed {
                correct: 3,
                total: 4
            }
        );
    }

    #[test]
    fn letter_answers_resolve_to_options() {
        let mut t = Scripted::new(&["I am a student", "bonjour", "bonjour", "suis"]);
        let out = run_stage(&mut t, &fixture_stage(), &RunOptions::default()).unwrap();
        assert_eq!(
            out,
            Outcome::Passed {
                correct: 4,
                total: 4
            }
        );
    }

    #[test]
    fn feedback_shows_expected_answer() {
        let mut t = Scripted::new(&["nope", "b", "bonjour", "suis"]);
        run_stage(&mut t, &fixture_stage(), &RunOptions::default()).unwrap();
        assert!(t
            .printed
            .iter()
            .any(|l| l.contains("expected: I am a student")));
    }

    #[test]
    fn accent_only_mismatch_is_correct_with_an_accent_hint() {
        // "etudiant" without the accent still passes — but the session
        // shows the accented form, so the learner learns the accent
        // (review item 3 / D1).
        let stage = Stage {
            id: 1,
            title: "Accents".into(),
            level_hint: "beginner".into(),
            vocabulary: vec![],
            exercises: vec![Exercise::Translation {
                prompt: "student?".into(),
                answer: "étudiant".into(),
                accepted_answers: vec![],
            }],
        };
        let mut t = Scripted::new(&["etudiant"]);
        let out = run_stage(&mut t, &stage, &RunOptions::default()).unwrap();
        assert_eq!(
            out,
            Outcome::Passed {
                correct: 1,
                total: 1
            }
        );
        assert!(t
            .printed
            .iter()
            .any(|l| l.contains("watch the accents: étudiant")));
    }

    fn vocab(word: &str, translation: &str) -> crate::engine::Vocabulary {
        crate::engine::Vocabulary {
            word: word.into(),
            translation: translation.into(),
            accepted_translations: vec![],
            pos: "noun".into(),
            example: format!("{word}!"),
        }
    }

    fn card_of<'a>(srs: &'a crate::srs::SrsState, word: &str) -> &'a crate::srs::Card {
        srs.cards().into_iter().find(|(w, _)| *w == word).unwrap().1
    }

    #[test]
    fn review_grades_and_reschedules_due_cards() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["bonjour".into(), "merci".into()], "2026-10-30");
        let v1 = vocab("bonjour", "hello");
        let v2 = vocab("merci", "thank you");
        let vocabulary = [&v1, &v2];

        let mut t = Scripted::new(&["hello", "nope"]);
        let out = run_review(&mut t, &mut srs, &vocabulary, "2026-10-30").unwrap();

        assert_eq!(
            out,
            ReviewOutcome {
                answered: 2,
                correct: 1
            }
        );
        assert_eq!(card_of(&srs, "bonjour").box_number, 2);
        assert_eq!(card_of(&srs, "bonjour").due, "2026-11-01");
        assert_eq!(card_of(&srs, "merci").box_number, 1);
        assert_eq!(card_of(&srs, "merci").due, "2026-10-31");
        assert!(t.printed.iter().any(|l| l.contains("expected: thank you")));
    }

    #[test]
    fn review_grading_is_case_insensitive() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["bonjour".into()], "2026-10-30");
        let v = vocab("bonjour", "hello");

        let mut t = Scripted::new(&["  HeLLo "]);
        let out = run_review(&mut t, &mut srs, &[&v], "2026-10-30").unwrap();
        assert_eq!(
            out,
            ReviewOutcome {
                answered: 1,
                correct: 1
            }
        );
    }

    #[test]
    fn review_input_closed_stops_gracefully() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["bonjour".into(), "merci".into()], "2026-10-30");
        let v1 = vocab("bonjour", "hello");
        let v2 = vocab("merci", "thank you");

        // only one answer scripted: the second read hits EOF and stops
        let mut t = Scripted::new(&["hello"]);
        let out = run_review(&mut t, &mut srs, &[&v1, &v2], "2026-10-30").unwrap();
        assert_eq!(
            out,
            ReviewOutcome {
                answered: 1,
                correct: 1
            }
        );
        assert!(t.printed.iter().any(|l| l.contains("input closed")));
        // the card never reached is untouched
        assert_eq!(card_of(&srs, "merci").due, "2026-10-30");
    }

    #[test]
    fn unreadable_input_aborts_a_stage_with_a_distinct_message() {
        let mut t = Scripted::with_inputs(vec![
            Input::Line("I am a student".into()),
            Input::Unreadable("stream did not contain valid UTF-8".into()),
        ]);
        let out = run_stage(&mut t, &fixture_stage(), &RunOptions::default()).unwrap();
        assert_eq!(
            out,
            Outcome::Failed {
                correct: 1,
                total: 4
            }
        );
        assert!(t.printed.iter().any(|l| l.contains("could not read input")));
        assert!(!t.printed.iter().any(|l| l.contains("input closed")));
    }

    #[test]
    fn unreadable_input_aborts_a_review_with_a_distinct_message() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["bonjour".into(), "merci".into()], "2026-10-30");
        let v1 = vocab("bonjour", "hello");
        let v2 = vocab("merci", "thank you");

        let mut t = Scripted::with_inputs(vec![
            Input::Line("hello".into()),
            Input::Unreadable("stream did not contain valid UTF-8".into()),
        ]);
        let out = run_review(&mut t, &mut srs, &[&v1, &v2], "2026-10-30").unwrap();
        assert_eq!(
            out,
            ReviewOutcome {
                answered: 1,
                correct: 1
            }
        );
        assert!(t.printed.iter().any(|l| l.contains("could not read input")));
        // the card never reached is untouched
        assert_eq!(card_of(&srs, "merci").due, "2026-10-30");
    }

    #[test]
    fn review_accepts_alternative_translations() {
        // The reported case: "salut" translates to "hi", but a learner
        // who answers "hello" is right too (D8).
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["salut".into()], "2026-10-30");
        let mut v = vocab("salut", "hi");
        v.accepted_translations = vec!["hello".into()];

        let mut t = Scripted::new(&["hello"]);
        let out = run_review(&mut t, &mut srs, &[&v], "2026-10-30").unwrap();
        assert_eq!(
            out,
            ReviewOutcome {
                answered: 1,
                correct: 1
            }
        );
        assert!(t.printed.iter().any(|l| l.contains("correct")));
        assert!(!t.printed.iter().any(|l| l.contains("not quite")));

        // and alternatives go through the same forgiving grading as the
        // primary translation — case and accents included (fresh
        // schedule: the card above is no longer due today)
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["salut".into()], "2026-10-30");
        let mut t = Scripted::new(&["  HeLLo "]);
        let out = run_review(&mut t, &mut srs, &[&v], "2026-10-30").unwrap();
        assert_eq!(out.correct, 1);
    }

    #[test]
    fn review_skips_cards_missing_from_content() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["orphelin".into()], "2026-10-30");

        let mut t = Scripted::new(&[]);
        let out = run_review(&mut t, &mut srs, &[], "2026-10-30").unwrap();
        assert_eq!(out, ReviewOutcome::default());
        assert!(t.printed.iter().any(|l| l.contains("skipping 'orphelin'")));
    }

    #[test]
    fn a_broken_pipe_aborts_the_stage_and_records_nothing() {
        // The pipe closes right after the two header lines: the first
        // exercise print fails and the whole run unwinds as an error.
        let mut p = LanguageProgress::new();
        let mut t = ClosedPipe { prints_left: 2 };
        let err = run_and_record(
            &mut t,
            &fixture_stage(),
            &mut p,
            "2026-08-22",
            &RunOptions::default(),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        // an aborted run is not a completed stage — nothing was saved
        assert!(!p.is_stage_completed(1));
        assert!(p.active_days().is_empty());
    }

    #[test]
    fn a_broken_pipe_aborts_the_review_and_answers_no_cards() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["bonjour".into(), "merci".into()], "2026-10-30");
        let v1 = vocab("bonjour", "hello");
        let v2 = vocab("merci", "thank you");

        // the pipe survives the header but dies on the first card
        let mut t = ClosedPipe { prints_left: 3 };
        let err = run_review(&mut t, &mut srs, &[&v1, &v2], "2026-10-30").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        // no card was answered, so the caller must not save the srs
        // state — the cards are still due today, untouched
        assert_eq!(srs.due_on("2026-10-30").len(), 2);
        assert_eq!(card_of(&srs, "bonjour").box_number, 1);
    }

    /// A stage whose every exercise answers to the same word, with
    /// distinct prompts — order is observable, correctness is not.
    fn uniform_stage(n: usize) -> Stage {
        Stage {
            id: 1,
            title: "Uniform".into(),
            level_hint: "beginner".into(),
            vocabulary: vec![],
            exercises: (0..n)
                .map(|i| Exercise::Translation {
                    prompt: format!("prompt {i}"),
                    answer: "a".into(),
                    accepted_answers: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn shuffle_runs_every_exercise_exactly_once() {
        // Whatever the seed, the run still asks all 6 exercises — same
        // multiset of prompts, just a different order (D7).
        let opts = RunOptions {
            order: ExerciseOrder::Shuffled(42),
        };
        let mut t = Scripted::new(&["a", "a", "a", "a", "a", "a"]);
        let out = run_stage(&mut t, &uniform_stage(6), &opts).unwrap();
        assert_eq!(
            out,
            Outcome::Passed {
                correct: 6,
                total: 6
            }
        );
        // sorted as multisets: same six prompts, whatever the order
        let mut asked: Vec<usize> = t
            .printed
            .iter()
            .filter(|l| l.contains("[Uniform — "))
            .map(|l| {
                l.rsplit("prompt ")
                    .next()
                    .and_then(|n| n.parse::<usize>().ok())
                    .expect("a numbered prompt line")
            })
            .collect();
        asked.sort();
        assert_eq!(asked, (0..6).collect::<Vec<_>>());
        // and the seed really shuffled for this run: not the authored order
        let asked_in_order: Vec<String> = t
            .printed
            .iter()
            .filter(|l| l.contains("[Uniform — "))
            .cloned()
            .collect();
        let authored: Vec<String> = (1..=6)
            .map(|i| format!("[Uniform — {i}/6] prompt {}", i - 1))
            .collect();
        assert_ne!(asked_in_order, authored);
    }

    #[test]
    fn same_seed_same_order_and_some_seed_moves_something() {
        // determinism: one seed, two runs, identical sequence
        let a = exercise_sequence(8, ExerciseOrder::Shuffled(7));
        let b = exercise_sequence(8, ExerciseOrder::Shuffled(7));
        assert_eq!(a, b);

        // a permutation, always: same elements, none lost
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(sorted, (0..8).collect::<Vec<_>>());

        // and among a handful of seeds at least one order differs from
        // the authored one — the point of shuffling (D7)
        let identity = (0..8).collect::<Vec<_>>();
        assert!((1..=10).any(|s| exercise_sequence(8, ExerciseOrder::Shuffled(s)) != identity));

        // in-order is the authored sequence, untouched
        assert_eq!(
            exercise_sequence(8, ExerciseOrder::InOrder),
            (0..8).collect::<Vec<_>>()
        );
    }
}
