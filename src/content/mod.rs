//! Content loading — turns `languages/<lang>/stages.json` into `engine::Stage`s.
//!
//! The `Json*` types here exist only to mirror the on-disk JSON shape so
//! serde can deserialize into them directly. They are then converted into
//! plain `engine::` types via `into_engine()`. `engine::` itself never
//! derives `Deserialize` — it stays storage-agnostic, per the project's
//! separation principle. When SQLite replaces JSON later, only this module
//! changes.
//!
//! Deserializing is not enough: a file can parse and still be unplayable
//! (a stage with no exercises can never be completed and blocks the
//! learner forever; a multiple-choice answer that is not an option can
//! never be picked). Every file therefore goes through [`validate`],
//! which checks all rules in one pass so an author fixes the whole file
//! at once instead of one error per run (review item 2).

use crate::engine;
use crate::lang::LangId;
use serde::Deserialize;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
struct JsonLanguageFile {
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
        /// Optional alternative spellings that also count as correct
        /// (review item 3 / D1). Absent in existing files = none.
        #[serde(default)]
        accepted_answers: Vec<String>,
    },
    MultipleChoice {
        prompt: String,
        options: Vec<String>,
        answer: String,
    },
    FillBlank {
        prompt: String,
        answer: String,
        #[serde(default)]
        accepted_answers: Vec<String>,
    },
}

/// A language's loaded content: the parsed stages plus the `language`
/// name declared in the file (which [`load_language`] has already checked
/// against the expected name, when one was given).
#[derive(Debug)]
pub struct LanguageContent {
    pub language: String,
    pub stages: Vec<engine::Stage>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    /// The file parsed, but broke one or more content rules. All
    /// violations are listed — a whole-file report, not the first error.
    Invalid(Vec<String>),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "could not read content file: {e}"),
            LoadError::Json(e) => write!(f, "could not parse content file: {e}"),
            LoadError::Invalid(violations) => {
                write!(f, "content file is not valid:")?;
                for v in violations {
                    write!(f, "\n  - {v}")?;
                }
                Ok(())
            }
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

/// Reads a `stages.json`-shaped file, converts it to `engine::Stage`s and
/// validates it.
///
/// `expected_language` is the name the file is being loaded as: the
/// content directory name for `start`/`progress`/…, the CLI argument for
/// `add-lang`. The file's `language` field must match it, so a directory
/// can never quietly serve content written for another language. Pass
/// `None` when there is no expected name (the `check` verb).
pub fn load_language<P: AsRef<Path>>(
    path: P,
    expected_language: Option<&LangId>,
) -> Result<LanguageContent, LoadError> {
    let text = fs::read_to_string(path)?;
    let parsed: JsonLanguageFile = serde_json::from_str(&text)?;
    let stages: Vec<engine::Stage> = parsed
        .stages
        .into_iter()
        .map(JsonStage::into_engine)
        .collect();
    let violations = validate(&stages, &parsed.language, expected_language);
    if violations.is_empty() {
        Ok(LanguageContent {
            language: parsed.language,
            stages,
        })
    } else {
        Err(LoadError::Invalid(violations))
    }
}

/// Every content rule, checked in one pass. Returns one human-readable
/// violation per broken rule — all of them, not just the first:
///
/// 1. at least one stage; stage ids unique and strictly ascending in
///    file order (so "the next stage" is well-defined);
/// 2. every stage has at least one exercise (an exercise-less stage can
///    never be completed and would block every stage after it);
/// 3. multiple choice: 2-26 options (letters a-z), all unique, answer
///    among them;
/// 4. no empty `prompt`/`answer` (exercises) or `word`/`translation`
///    (vocabulary) — whitespace-only counts as empty;
/// 5. the `language` field is non-empty and matches `expected_language`;
/// 6. fill-blank prompts contain a `___` blank.
pub fn validate(
    stages: &[engine::Stage],
    language: &str,
    expected_language: Option<&LangId>,
) -> Vec<String> {
    let mut errors = Vec::new();

    if stages.is_empty() {
        errors.push("the file must contain at least one stage".to_string());
    }

    // Rule 1 — ids strictly ascending in file order (also catches duplicates).
    let mut highest_id: Option<u32> = None;
    for (position, stage) in stages.iter().enumerate() {
        if let Some(prev) = highest_id {
            if stage.id <= prev {
                errors.push(format!(
                    "stage {} (position {}): stage ids must be unique and strictly \
                     ascending in file order — it comes after stage {prev}",
                    stage.id,
                    position + 1
                ));
            }
        }
        highest_id = Some(highest_id.map_or(stage.id, |prev| prev.max(stage.id)));
    }

    for stage in stages {
        // Rule 2 — a stage must be playable.
        if stage.exercises.is_empty() {
            errors.push(format!(
                "stage {} ({}): no exercises — an empty stage can never reach the \
                 80% bar, so the learner would be stuck on it forever",
                stage.id, stage.title
            ));
        }

        // Rule 4 — vocabulary fields.
        for (n, v) in stage.vocabulary.iter().enumerate() {
            if v.word.trim().is_empty() {
                errors.push(format!(
                    "stage {}, vocabulary entry {}: word must not be empty",
                    stage.id,
                    n + 1
                ));
            }
            if v.translation.trim().is_empty() {
                errors.push(format!(
                    "stage {}, vocabulary entry {}: translation must not be empty",
                    stage.id,
                    n + 1
                ));
            }
        }

        for (n, exercise) in stage.exercises.iter().enumerate() {
            validate_exercise(stage.id, n + 1, exercise, &mut errors);
        }
    }

    // Rule 5 — the language field.
    if language.trim().is_empty() {
        errors.push("the \"language\" field must not be empty".to_string());
    } else if let Some(expected) = expected_language {
        if language != expected.as_str() {
            errors.push(format!(
                "the file declares language \"{language}\" but it is being loaded \
                 as \"{}\"",
                expected.as_str()
            ));
        }
    }

    errors
}

fn validate_exercise(
    stage_id: u32,
    n: usize,
    exercise: &engine::Exercise,
    errors: &mut Vec<String>,
) {
    let kind = exercise_kind(exercise);
    let answer = exercise.answer();
    if exercise.prompt().trim().is_empty() {
        errors.push(format!(
            "stage {stage_id}, exercise {n} ({kind}): prompt must not be empty"
        ));
    }
    if answer.trim().is_empty() {
        errors.push(format!(
            "stage {stage_id}, exercise {n} ({kind}): answer must not be empty"
        ));
    }
    for (m, alt) in exercise.accepted_answers().iter().enumerate() {
        if alt.trim().is_empty() {
            errors.push(format!(
                "stage {stage_id}, exercise {n} ({kind}): accepted_answers entry {} must not be empty",
                m + 1
            ));
        }
    }
    match exercise {
        engine::Exercise::MultipleChoice { options, .. } => {
            // Rule 3 — options and answer.
            if !(2..=26).contains(&options.len()) {
                errors.push(format!(
                    "stage {stage_id}, exercise {n} (multiple_choice): needs between \
                     2 and 26 options, found {}",
                    options.len()
                ));
            }
            for (i, opt) in options.iter().enumerate() {
                if options[..i].contains(opt) {
                    errors.push(format!(
                        "stage {stage_id}, exercise {n} (multiple_choice): duplicate \
                         option {opt:?}"
                    ));
                }
            }
            if !answer.trim().is_empty() && !options.iter().any(|o| o == answer) {
                errors.push(format!(
                    "stage {stage_id}, exercise {n} (multiple_choice): answer {answer:?} \
                     is not one of the options"
                ));
            }
        }
        engine::Exercise::FillBlank { prompt, .. } => {
            // Rule 6 — there must be a blank to fill.
            if !prompt.contains("___") {
                errors.push(format!(
                    "stage {stage_id}, exercise {n} (fill_blank): prompt must contain \
                     \"___\" — the blank to fill"
                ));
            }
        }
        engine::Exercise::Translation { .. } => {}
    }
}

fn exercise_kind(exercise: &engine::Exercise) -> &'static str {
    match exercise {
        engine::Exercise::Translation { .. } => "translation",
        engine::Exercise::MultipleChoice { .. } => "multiple_choice",
        engine::Exercise::FillBlank { .. } => "fill_blank",
    }
}

impl JsonStage {
    fn into_engine(self) -> engine::Stage {
        engine::Stage {
            id: self.id,
            title: self.title,
            level_hint: self.level_hint,
            vocabulary: self
                .vocabulary
                .into_iter()
                .map(JsonVocabulary::into_engine)
                .collect(),
            exercises: self
                .exercises
                .into_iter()
                .map(JsonExercise::into_engine)
                .collect(),
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
            JsonExercise::Translation {
                prompt,
                answer,
                accepted_answers,
            } => engine::Exercise::Translation {
                prompt,
                answer,
                accepted_answers,
            },
            JsonExercise::MultipleChoice {
                prompt,
                options,
                answer,
            } => engine::Exercise::MultipleChoice {
                prompt,
                options,
                answer,
            },
            JsonExercise::FillBlank {
                prompt,
                answer,
                accepted_answers,
            } => engine::Exercise::FillBlank {
                prompt,
                answer,
                accepted_answers,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A scratch JSON file that removes itself when dropped.
    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn new(text: &str) -> Self {
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "termocab-content-{}-{}.json",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&path, text).unwrap();
            Self(path)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn french() -> LangId {
        LangId::parse("french").unwrap()
    }

    const VOCAB_OK: &str = r#"{ "word": "bonjour", "translation": "hello", "pos": "interjection", "example": "Bonjour !" }"#;
    const TR_OK: &str = r#"{ "type": "translation", "prompt": "bonjour", "answer": "hello" }"#;
    const MC_OK: &str = r#"{ "type": "multiple_choice", "prompt": "pick hello", "options": ["hello", "bye", "nope"], "answer": "hello" }"#;
    const FB_OK: &str =
        r#"{ "type": "fill_blank", "prompt": "___ means hello", "answer": "bonjour" }"#;

    /// A stage that satisfies every rule.
    fn good_stage(id: u32) -> String {
        stage_json(id, VOCAB_OK, &format!("{TR_OK}, {MC_OK}, {FB_OK}"))
    }

    fn stage_json(id: u32, vocabulary: &str, exercises: &str) -> String {
        format!(
            r#"{{ "id": {id}, "title": "Stage {id}", "level_hint": "beginner",
                "vocabulary": [{vocabulary}], "exercises": [{exercises}] }}"#
        )
    }

    fn file_of(language: &str, stages: &str) -> String {
        format!(r#"{{ "language": "{language}", "stages": [{stages}] }}"#)
    }

    fn load_text(text: &str, expected: Option<&LangId>) -> Result<LanguageContent, LoadError> {
        let file = TempFile::new(text);
        load_language(&file.0, expected)
    }

    fn violations_of(text: &str, expected: Option<&LangId>) -> Vec<String> {
        match load_text(text, expected) {
            Err(LoadError::Invalid(v)) => v,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn valid_file_loads() {
        let loaded = load_text(&file_of("french", &good_stage(1)), Some(&french())).unwrap();
        assert_eq!(loaded.language, "french");
        assert_eq!(loaded.stages.len(), 1);
        assert_eq!(loaded.stages[0].exercises.len(), 3);
        assert_eq!(loaded.stages[0].vocabulary.len(), 1);
    }

    #[test]
    fn zero_stages_is_rejected() {
        let v = violations_of(&file_of("french", ""), None);
        assert!(v.iter().any(|e| e.contains("at least one stage")));
    }

    #[test]
    fn stage_ids_must_be_unique_and_ascending() {
        let v = violations_of(
            &file_of("french", &format!("{},{}", good_stage(1), good_stage(1))),
            None,
        );
        assert!(v
            .iter()
            .any(|e| e.contains("unique and strictly ascending")));

        let v = violations_of(
            &file_of("french", &format!("{},{}", good_stage(2), good_stage(1))),
            None,
        );
        assert!(v
            .iter()
            .any(|e| e.contains("unique and strictly ascending")));
    }

    #[test]
    fn empty_stage_is_rejected_and_named() {
        let v = violations_of(&file_of("french", &stage_json(7, VOCAB_OK, "")), None);
        assert!(v
            .iter()
            .any(|e| e.contains("stage 7") && e.contains("no exercises")));
    }

    #[test]
    fn multiple_choice_answer_must_be_an_option() {
        let bad_mc = r#"{ "type": "multiple_choice", "prompt": "pick", "options": ["x", "y"], "answer": "z" }"#;
        let v = violations_of(&file_of("french", &stage_json(1, VOCAB_OK, bad_mc)), None);
        assert!(v
            .iter()
            .any(|e| e.contains("answer \"z\" is not one of the options")));
    }

    #[test]
    fn multiple_choice_option_count_is_bounded() {
        for options in [
            r#"["only"]"#,
            r#"["a","b","c","d","e","f","g","h","i","j","k","l","m","n","o","p","q","r","s","t","u","v","w","x","y","z","aa"]"#,
        ] {
            let bad_mc = format!(
                r#"{{ "type": "multiple_choice", "prompt": "pick", "options": {options}, "answer": "a" }}"#
            );
            let v = violations_of(&file_of("french", &stage_json(1, VOCAB_OK, &bad_mc)), None);
            assert!(
                v.iter().any(|e| e.contains("between 2 and 26 options")),
                "options {options} should be rejected"
            );
        }
    }

    #[test]
    fn multiple_choice_options_must_be_unique() {
        let bad_mc = r#"{ "type": "multiple_choice", "prompt": "pick", "options": ["x", "x"], "answer": "x" }"#;
        let v = violations_of(&file_of("french", &stage_json(1, VOCAB_OK, bad_mc)), None);
        assert!(v.iter().any(|e| e.contains("duplicate option")));
    }

    #[test]
    fn empty_fields_are_rejected() {
        let exercises = r#"
            { "type": "translation", "prompt": "  ", "answer": "hello" },
            { "type": "fill_blank", "prompt": "___ means hello", "answer": "" }"#;
        let vocab = r#"{ "word": "", "translation": "hello", "pos": "x", "example": "e" },
            { "word": "salut", "translation": " ", "pos": "x", "example": "e" }"#;
        let v = violations_of(&file_of("french", &stage_json(1, vocab, exercises)), None);
        assert!(v.iter().any(|e| e.contains("prompt must not be empty")));
        assert!(v.iter().any(|e| e.contains("answer must not be empty")));
        assert!(v.iter().any(|e| e.contains("word must not be empty")));
        assert!(v
            .iter()
            .any(|e| e.contains("translation must not be empty")));
    }

    #[test]
    fn language_field_must_match_expected_name() {
        let v = violations_of(&file_of("italian", &good_stage(1)), Some(&french()));
        assert!(v
            .iter()
            .any(|e| e.contains("declares language \"italian\"") && e.contains("\"french\"")));

        let v = violations_of(&file_of("", &good_stage(1)), None);
        assert!(v
            .iter()
            .any(|e| e.contains("\"language\" field must not be empty")));
    }

    #[test]
    fn no_expected_name_skips_the_match_check() {
        let loaded = load_text(&file_of("italian", &good_stage(1)), None).unwrap();
        assert_eq!(loaded.language, "italian");
    }

    #[test]
    fn fill_blank_needs_a_blank() {
        let bad_fb =
            r#"{ "type": "fill_blank", "prompt": "bonjour means hello", "answer": "hello" }"#;
        let v = violations_of(&file_of("french", &stage_json(1, VOCAB_OK, bad_fb)), None);
        assert!(v.iter().any(|e| e.contains("must contain \"___\"")));
    }

    #[test]
    fn accepted_answers_are_optional_but_must_be_nonempty_strings() {
        // absent is fine (backwards compatible)
        load_text(&file_of("french", &good_stage(1)), Some(&french())).unwrap();
        // an empty entry is a violation
        let bad = r#"{ "type": "translation", "prompt": "bonjour", "answer": "hello", "accepted_answers": [""] }"#;
        let v = violations_of(&file_of("french", &stage_json(1, VOCAB_OK, bad)), None);
        assert!(v
            .iter()
            .any(|e| e.contains("accepted_answers entry 1 must not be empty")));
    }

    #[test]
    fn all_violations_are_reported_at_once() {
        // Two broken exercises AND a language mismatch: all three surface.
        let exercises = r#"
            { "type": "multiple_choice", "prompt": "pick", "options": ["x", "y"], "answer": "z" },
            { "type": "fill_blank", "prompt": "no blank here", "answer": "hello" }"#;
        let v = violations_of(
            &file_of("italian", &stage_json(1, VOCAB_OK, exercises)),
            Some(&french()),
        );
        assert_eq!(v.len(), 3, "every violation should be reported: {v:?}");
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let result = load_text("{ this is not json", None);
        assert!(matches!(result, Err(LoadError::Json(_))));
    }

    #[test]
    fn unknown_exercise_type_is_a_parse_error() {
        let bad = r#"{ "type": "matching", "prompt": "x", "answer": "y" }"#;
        let result = load_text(&file_of("french", &stage_json(1, VOCAB_OK, bad)), None);
        assert!(matches!(result, Err(LoadError::Json(_))));
    }

    #[test]
    fn missing_field_is_a_parse_error() {
        let bad = r#"{ "type": "translation", "prompt": "bonjour" }"#; // no answer
        let result = load_text(&file_of("french", &stage_json(1, VOCAB_OK, bad)), None);
        assert!(matches!(result, Err(LoadError::Json(_))));
    }
}
