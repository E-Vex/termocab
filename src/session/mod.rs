//! Session runner — the application layer between the CLI and the
//! engine. Runs one stage's exercises in the terminal, grades them
//! through `engine::`, applies the >= 80% completion rule, and records
//! the outcome via `progress::`. It talks to the user through the
//! `Terminal` trait so unit tests can drive it with a scripted player
//! instead of a real stdin/stdout.

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
        out.push_str("\n");
        for (i, opt) in options.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", (b'a' + i as u8) as char, opt));
        }
        out.push_str("(answer with a letter or the word itself)");
    }
    out
}

/// Resolves a multiple-choice attempt: 'b' picks the second option; any
/// other text is compared against the answer directly.
fn resolve_attempt(exercise: &Exercise, raw: &str) -> String {
    if let Exercise::MultipleChoice { options, .. } = exercise {
        let t = raw.trim();
        if t.len() == 1 {
            let c = t.chars().next().expect("len 1").to_ascii_lowercase();
            if let Some(idx) = c.to_digit(36).map(|d| d.wrapping_sub(10)) {
                if (idx as usize) < options.len() {
                    return options[idx as usize].clone();
                }
            }
        }
    }
    raw.to_string()
}

/// Runs every exercise in the stage once, in order, then reports the
/// outcome. Pure with respect to storage: the caller decides what to do
/// with the result.
pub fn run_stage<T: Terminal>(term: &mut T, stage: &Stage) -> Outcome {
    let total = stage.exercises.len();
    let mut correct = 0usize;
    term.print(&format!("— Stage {}: {} —", stage.id, stage.title));
    term.print(&format!("{total} exercise(s). Answer each, then press Enter."));

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
        assert_eq!(out, Outcome::Passed { correct: 4, total: 4 });
        assert!(p.is_stage_completed(1));
        assert_eq!(p.active_days(), &["2026-08-22".to_string()]);
    }

    #[test]
    fn grading_is_case_insensitive() {
        let mut t = Scripted::new(&["i AM a student", "b", "  bonjour  ", "SUIS"]);
        let out = run_stage(&mut t, &fixture_stage());
        assert_eq!(out, Outcome::Passed { correct: 4, total: 4 });
    }

    #[test]
    fn three_of_four_is_below_the_bar() {
        // 3/4 = 75% — just under the 80% bar.
        let mut t = Scripted::new(&["wrong", "b", "bonjour", "suis"]);
        let out = run_stage(&mut t, &fixture_stage());
        assert_eq!(out, Outcome::Failed { correct: 3, total: 4 });
    }

    #[test]
    fn letter_answers_resolve_to_options() {
        let mut t = Scripted::new(&["I am a student", "bonjour", "bonjour", "suis"]);
        let out = run_stage(&mut t, &fixture_stage());
        assert_eq!(out, Outcome::Passed { correct: 4, total: 4 });
    }

    #[test]
    fn feedback_shows_expected_answer() {
        let mut t = Scripted::new(&["nope", "b", "bonjour", "suis"]);
        run_stage(&mut t, &fixture_stage());
        assert!(t.printed.iter().any(|l| l.contains("expected: I am a student")));
    }
}
