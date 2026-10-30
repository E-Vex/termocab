//! Session runner — the application layer between the CLI and the
//! engine. Runs one stage's exercises in the terminal, grades them
//! through `engine::`, applies the >= 80% completion rule, and records
//! the outcome via `progress::`. Runs spaced-repetition review sessions
//! the same way: due cards in, rescheduled cards out, grading through
//! `engine::`. It talks to the user through the `Terminal` trait so
//! unit tests can drive it with a scripted player instead of a real
//! stdin/stdout.

use crate::engine::{Exercise, Stage};
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

/// Minimal terminal the runner needs. Tests implement it with a queue of
/// pre-scripted answers; real usage uses `StdioTerminal`.
pub trait Terminal {
    fn print(&mut self, line: &str);
    /// Reads one line of user input. `None` means EOF / input closed.
    fn read_line(&mut self) -> Option<String>;
}

/// The real terminal: stdout + stdin.
pub struct StdioTerminal;

impl Terminal for StdioTerminal {
    fn print(&mut self, line: &str) {
        println!("{line}");
        let _ = io::stdout().flush();
    }

    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => Some(line),
            Err(_) => None,
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

/// Runs every exercise in the stage once, in order, then reports the
/// outcome. Pure with respect to storage: the caller decides what to do
/// with the result.
pub fn run_stage<T: Terminal>(term: &mut T, stage: &Stage) -> Outcome {
    let total = stage.exercises.len();
    let mut correct = 0usize;
    term.print(&format!("— Stage {}: {} —", stage.id, stage.title));
    term.print(&format!(
        "{total} exercise(s). Answer each, then press Enter."
    ));

    for (i, exercise) in stage.exercises.iter().enumerate() {
        term.print("");
        term.print(&render(&stage.title, i + 1, total, exercise));
        let answer = match term.read_line() {
            Some(line) => resolve_attempt(exercise, &line),
            None => {
                term.print("input closed — stopping this stage.");
                break;
            }
        };
        if exercise.is_correct(&answer) {
            correct += 1;
            term.print("correct.");
        } else {
            term.print(&format!("not quite — expected: {}", exercise.answer()));
        }
    }

    let outcome = if stage.is_completed_by(correct) {
        Outcome::Passed { correct, total }
    } else {
        Outcome::Failed { correct, total }
    };
    term.print("");
    match outcome {
        Outcome::Passed { correct, total } => {
            term.print(&format!("stage complete: {correct}/{total} correct. nice."))
        }
        Outcome::Failed { correct, total } => term.print(&format!(
            "{correct}/{total} correct — below the 80% bar. run it again when ready."
        )),
    }
    outcome
}

/// Convenience wrapper: run the stage, and if it passed, record it in
/// progress with the given date (the caller decides what "today" is).
pub fn run_and_record<T: Terminal>(
    term: &mut T,
    stage: &Stage,
    progress: &mut LanguageProgress,
    today: &str,
) -> Outcome {
    let outcome = run_stage(term, stage);
    if let Outcome::Passed { .. } = outcome {
        progress.record_stage_completion(stage.id, today);
    }
    outcome
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
) -> ReviewOutcome {
    let due: Vec<String> = srs
        .due_on(today)
        .into_iter()
        .map(|(w, _)| w.to_string())
        .collect();
    let total = due.len();
    let mut outcome = ReviewOutcome::default();

    term.print(&format!("— review: {total} word(s) —"));
    term.print("give the translation of each word, then press Enter.");

    for (i, word) in due.iter().enumerate() {
        let Some(v) = vocabulary.iter().find(|v| v.word == *word) else {
            term.print(&format!(
                "skipping '{word}' — no longer in the loaded content."
            ));
            continue;
        };
        term.print("");
        term.print(&format!("[{}/{}] {}", i + 1, total, v.word));
        let attempt = match term.read_line() {
            Some(line) => line,
            None => {
                term.print("input closed — stopping this review.");
                break;
            }
        };
        // a review card IS a translation exercise: this reuses the
        // engine's grading (case- and whitespace-insensitive) instead
        // of growing a second, slightly different rule here
        let exercise = Exercise::Translation {
            prompt: v.word.clone(),
            answer: v.translation.clone(),
        };
        outcome.answered += 1;
        if exercise.is_correct(&attempt) {
            outcome.correct += 1;
            match srs.answer(word, true, today) {
                Some(s) => term.print(&format!("correct — box {}, due {}.", s.box_number, s.due)),
                None => term.print("correct."),
            }
        } else {
            match srs.answer(word, false, today) {
                Some(s) => term.print(&format!(
                    "not quite — expected: {}. back to box {}, due {}.",
                    v.translation, s.box_number, s.due
                )),
                None => term.print(&format!("not quite — expected: {}.", v.translation)),
            }
        }
    }

    term.print("");
    term.print(&format!(
        "reviewed {} word(s): {} correct, {} to revisit.",
        outcome.answered,
        outcome.correct,
        outcome.answered - outcome.correct
    ));
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player with scripted answers, capturing everything printed.
    struct Scripted {
        answers: Vec<String>,
        printed: Vec<String>,
        at: usize,
    }

    impl Scripted {
        fn new(answers: &[&str]) -> Self {
            Self {
                answers: answers.iter().map(|s| s.to_string()).collect(),
                printed: vec![],
                at: 0,
            }
        }
    }

    impl Terminal for Scripted {
        fn print(&mut self, line: &str) {
            self.printed.push(line.to_string());
        }
        fn read_line(&mut self) -> Option<String> {
            let a = self.answers.get(self.at).cloned();
            self.at += 1;
            a
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
                },
                Exercise::MultipleChoice {
                    prompt: "Choose the correct greeting:".into(),
                    options: vec!["maison".into(), "bonjour".into(), "manger".into()],
                    answer: "bonjour".into(),
                },
                Exercise::FillBlank {
                    prompt: "___, comment ca va ?".into(),
                    answer: "Bonjour".into(),
                },
                Exercise::FillBlank {
                    prompt: "Je ___ etudiant.".into(),
                    answer: "suis".into(),
                },
            ],
        }
    }

    #[test]
    fn all_correct_passes_and_records() {
        let mut p = LanguageProgress::new();
        let mut t = Scripted::new(&["I am a student", "b", "bonjour", "suis"]);
        let out = run_and_record(&mut t, &fixture_stage(), &mut p, "2026-08-22");
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
        let out = run_stage(&mut t, &fixture_stage());
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
        let out = run_stage(&mut t, &fixture_stage());
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
        let out = run_stage(&mut t, &fixture_stage());
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
        run_stage(&mut t, &fixture_stage());
        assert!(t
            .printed
            .iter()
            .any(|l| l.contains("expected: I am a student")));
    }

    fn vocab(word: &str, translation: &str) -> crate::engine::Vocabulary {
        crate::engine::Vocabulary {
            word: word.into(),
            translation: translation.into(),
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
        let out = run_review(&mut t, &mut srs, &vocabulary, "2026-10-30");

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
        let out = run_review(&mut t, &mut srs, &[&v], "2026-10-30");
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
        let out = run_review(&mut t, &mut srs, &[&v1, &v2], "2026-10-30");
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
    fn review_skips_cards_missing_from_content() {
        let mut srs = crate::srs::SrsState::new();
        srs.introduce(&["orphelin".into()], "2026-10-30");

        let mut t = Scripted::new(&[]);
        let out = run_review(&mut t, &mut srs, &[], "2026-10-30");
        assert_eq!(out, ReviewOutcome::default());
        assert!(t.printed.iter().any(|l| l.contains("skipping 'orphelin'")));
    }
}
