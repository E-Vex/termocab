//! Learning Engine — domain types only.
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
