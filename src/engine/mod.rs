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
