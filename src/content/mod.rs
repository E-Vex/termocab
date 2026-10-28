//! Content loading — turns `languages/<lang>/stages.json` into `engine::Stage`s.
//!
//! The `Json*` types here exist only to mirror the on-disk JSON shape so
//! serde can deserialize into them directly. They are then converted into
//! plain `engine::` types via `into_engine()`. `engine::` itself never
//! derives `Deserialize` — it stays storage-agnostic, per the project's
//! separation principle. When SQLite replaces JSON later, only this module
//! changes.

use crate::engine;
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct JsonLanguageFile {
    #[allow(dead_code)] // not consumed yet — will matter once multiple languages coexist
    language: String,
    stages: Vec<JsonStage>,
}

#[derive(Deserialize)]
struct JsonStage {
    id: u32,
    title: String,
    level_hint: String,
    vocabulary: Vec<JsonVocabulary>,
    exercises: Vec<JsonExercise>,
}

#[derive(Deserialize)]
struct JsonVocabulary {
    word: String,
    translation: String,
    pos: String,
    example: String,
}

/// `#[serde(tag = "type")]` reads the JSON "type" field to pick the variant —
/// this is the direct Rust answer to the "exercises aren't all the same
/// shape" problem: the compiler enforces the tag/data pairing, not a
/// convention you have to remember.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum JsonExercise {
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

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "could not read content file: {e}"),
            LoadError::Json(e) => write!(f, "could not parse content file: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        LoadError::Io(e)
    }
}

impl From<serde_json::Error> for LoadError {
    fn from(e: serde_json::Error) -> Self {
        LoadError::Json(e)
    }
}

/// Reads a `stages.json`-shaped file and returns the stages as plain
/// `engine::Stage`s, ready for the engine to use.
pub fn load_language<P: AsRef<Path>>(path: P) -> Result<Vec<engine::Stage>, LoadError> {
    let text = fs::read_to_string(path)?;
    let parsed: JsonLanguageFile = serde_json::from_str(&text)?;
    Ok(parsed.stages.into_iter().map(JsonStage::into_engine).collect())
}

impl JsonStage {
    fn into_engine(self) -> engine::Stage {
        engine::Stage {
            id: self.id,
            title: self.title,
            level_hint: self.level_hint,
            vocabulary: self.vocabulary.into_iter().map(JsonVocabulary::into_engine).collect(),
            exercises: self.exercises.into_iter().map(JsonExercise::into_engine).collect(),
        }
    }
}

impl JsonVocabulary {
    fn into_engine(self) -> engine::Vocabulary {
        engine::Vocabulary {
            word: self.word,
            translation: self.translation,
            pos: self.pos,
            example: self.example,
        }
    }
}

impl JsonExercise {
    fn into_engine(self) -> engine::Exercise {
        match self {
            JsonExercise::Translation { prompt, answer } => {
                engine::Exercise::Translation { prompt, answer }
            }
            JsonExercise::MultipleChoice { prompt, options, answer } => {
                engine::Exercise::MultipleChoice { prompt, options, answer }
            }
            JsonExercise::FillBlank { prompt, answer } => {
                engine::Exercise::FillBlank { prompt, answer }
            }
        }
    }
}
