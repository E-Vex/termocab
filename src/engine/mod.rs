//! Learning Engine — domain types and pure learning rules.
//!
//! Nothing in this module knows that content ever came from JSON, or that
//! progress will ever be written to a file. It only deals with in-memory
//! shapes. `content::` is the only module allowed to know about `engine::`
//! types AND about JSON at the same time.

#[derive(Debug)]
pub struct Stage {
    pub id: u32,
    pub title: String,
    pub level_hint: String,
    pub vocabulary: Vec<Vocabulary>,
    pub exercises: Vec<Exercise>,
}

#[derive(Debug)]
pub struct Vocabulary {
    pub word: String,
    pub translation: String,
    pub pos: String,
    pub example: String,
}

/// The three MVP exercise kinds. Each variant only carries the fields that
/// kind actually needs — a `MultipleChoice` without an `options` field is
/// not representable, so there's nothing to check for at runtime.
#[derive(Debug)]
pub enum Exercise {
    Translation {
        prompt: String,
        answer: String,
    },
    MultipleChoice {
        prompt: String,
        options: Vec<String>,
        answer: String,
    },
    FillBlank {
        prompt: String,
        answer: String,
    },
}

impl Exercise {
    /// The text shown to the learner for this exercise, as-is.
    pub fn prompt(&self) -> &str {
        match self {
            Exercise::Translation { prompt, .. }
            | Exercise::MultipleChoice { prompt, .. }
            | Exercise::FillBlank { prompt, .. } => prompt,
        }
    }

    /// The canonical expected answer, regardless of exercise kind.
    pub fn answer(&self) -> &str {
        match self {
            Exercise::Translation { answer, .. }
            | Exercise::MultipleChoice { answer, .. }
            | Exercise::FillBlank { answer, .. } => answer,
        }
    }

    /// Grades a learner's attempt. Case-insensitive and whitespace-
    /// insensitive: "bonjour", "Bonjour" and "  bonjour " are all correct
    /// for answer "Bonjour". The three MVP exercise kinds share this one
    /// rule — there is deliberately no kind-specific grading yet.
    pub fn is_correct(&self, attempt: &str) -> bool {
        normalize(attempt) == normalize(self.answer())
    }
}

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// A stage is completed when the learner answered at least this
/// percentage of its exercises correctly in one run. Set at 80% per the
/// handoff's unlock-condition decision — 7 of 8 exercises, 5 of 6, and
/// so on. Kept as an integer percentage so the comparison is exact
/// integer math, not a float equality on the pass bar.
pub const STAGE_PASS_PERCENT: u32 = 80;

impl Stage {
    /// Whether a run of `correct` out of this stage's exercises completes
    /// the stage. A stage with no exercises can never be completed
    /// (content validation rejects such stages; this stays defensive).
    pub fn is_completed_by(&self, correct: usize) -> bool {
        let total = self.exercises.len();
        total > 0 && correct * 100 >= total * STAGE_PASS_PERCENT as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage_with(exercise_count: usize) -> Stage {
        Stage {
            id: 1,
            title: "Thresholds".into(),
            level_hint: "beginner".into(),
            vocabulary: vec![],
            exercises: (0..exercise_count)
                .map(|_| Exercise::Translation {
                    prompt: "prompt".into(),
                    answer: "answer".into(),
                })
                .collect(),
        }
    }

    #[test]
    fn pass_bar_sits_at_exactly_80_percent() {
        // 4/5 = 80% exactly — passes, the >= matters.
        assert!(stage_with(5).is_completed_by(4));
        // The handoff's own examples: 7 of 8 and 5 of 6 pass…
        assert!(stage_with(8).is_completed_by(7));
        assert!(stage_with(6).is_completed_by(5));
        // …4 of 6 (~67%) and 3/4 (75%) do not.
        assert!(!stage_with(6).is_completed_by(4));
        assert!(!stage_with(4).is_completed_by(3));
        // All correct always passes (except the empty stage below).
        assert!(stage_with(3).is_completed_by(3));
        assert!(stage_with(100).is_completed_by(80));
        assert!(!stage_with(100).is_completed_by(79));
    }

    #[test]
    fn a_stage_with_no_exercises_never_completes() {
        // Content validation keeps such stages out of the files; the
        // engine stays defensive anyway (review item 1's root cause).
        assert!(!stage_with(0).is_completed_by(0));
    }

    #[test]
    fn grading_is_case_and_whitespace_insensitive() {
        let ex = Exercise::Translation {
            prompt: "Bonjour".into(),
            answer: "Bonjour".into(),
        };
        assert!(ex.is_correct("bonjour"));
        assert!(ex.is_correct("  BONJOUR  "));
        assert!(ex.is_correct("Bonjour"));
        assert!(!ex.is_correct("salut"));
        assert!(!ex.is_correct(""));
    }

    #[test]
    fn prompt_and_answer_accessors_cover_every_kind() {
        let mc = Exercise::MultipleChoice {
            prompt: "pick one".into(),
            options: vec!["x".into(), "y".into()],
            answer: "x".into(),
        };
        let fb = Exercise::FillBlank {
            prompt: "___ means hello".into(),
            answer: "bonjour".into(),
        };
        assert_eq!(mc.prompt(), "pick one");
        assert_eq!(mc.answer(), "x");
        assert_eq!(fb.prompt(), "___ means hello");
        assert_eq!(fb.answer(), "bonjour");
    }
}
