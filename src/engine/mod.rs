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
    /// Alternative translations that also count as correct in a
    /// review: "salut" is "hi", but a learner who types "hello" is
    /// right too. Same pattern as `accepted_answers` on exercises
    /// (decision D8); absent in a file means "only the translation".
    pub accepted_translations: Vec<String>,
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
        /// Alternative spellings that also count as correct, e.g.
        /// "I'm a student" for "I am a student". Optional in content
        /// files; absent means "only the answer".
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
        accepted_answers: Vec<String>,
    },
}

/// The outcome of grading one attempt (review item 3 / decision D1).
#[derive(Debug, PartialEq, Eq)]
pub enum Grade {
    /// Right, after normalization.
    Correct,
    /// Right except for accents or ligatures — it counts as correct,
    /// but the learner should see the properly spelled form, so the
    /// expected spelling rides along as a hint.
    CorrectWithHint(String),
    Incorrect,
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

    /// Alternative accepted spellings (empty for multiple choice, which
    /// has its options instead).
    pub fn accepted_answers(&self) -> &[String] {
        match self {
            Exercise::Translation {
                accepted_answers, ..
            }
            | Exercise::FillBlank {
                accepted_answers, ..
            } => accepted_answers,
            Exercise::MultipleChoice { .. } => &[],
        }
    }

    /// Grades an attempt against the answer and its accepted
    /// alternatives. See [`normalize_strict`] for what "the same" means.
    pub fn grade(&self, attempt: &str) -> Grade {
        let candidates: Vec<&str> = once(self.answer())
            .chain(self.accepted_answers().iter().map(String::as_str))
            .collect();

        // Strict first: identical after normalization.
        let strict = normalize_strict(attempt);
        for candidate in &candidates {
            if normalize_strict(candidate) == strict {
                return Grade::Correct;
            }
        }
        // Loose second: same except accents/ligatures — correct, with
        // the properly spelled form as a hint.
        let loose = normalize_loose(attempt);
        for candidate in &candidates {
            if normalize_loose(candidate) == loose {
                return Grade::CorrectWithHint((*candidate).to_string());
            }
        }
        Grade::Incorrect
    }

    /// Whether an attempt counts toward the pass bar. An accent-only
    /// mismatch does (the hint is pedagogical, not punitive).
    pub fn is_correct(&self, attempt: &str) -> bool {
        !matches!(self.grade(attempt), Grade::Incorrect)
    }
}

use std::iter::once;

/// The accented letters of the Latin-1 Supplement — the Western
/// European set (French, Spanish, German, Italian, Portuguese, …),
/// decomposed to base letter + combining mark. Decomposing (rather
/// than stripping) in the strict pass makes precomposed "é" equal to
/// "e" + U+0301 typed on a macOS keyboard — same letter, same accent,
/// different encoding. Hand-written on purpose: twenty-seven entries
/// beat a Unicode-normalization crate, and §6 asks for justification
/// before any dependency. When a language outside this set arrives
/// (Polish ł, Romanian ș, …) extend it with Latin Extended-A entries;
/// letters that are letters in their own right (ø, þ, ð) never fold —
/// "smør" is not "smor". Lowercase only: [`normalize_strict`]
/// lowercases before it looks here (D9).
const ACCENTED: [(char, &str); 27] = [
    ('à', "a\u{0300}"),
    ('á', "a\u{0301}"),
    ('â', "a\u{0302}"),
    ('ã', "a\u{0303}"),
    ('ä', "a\u{0308}"),
    ('å', "a\u{030A}"),
    ('ç', "c\u{0327}"),
    ('é', "e\u{0301}"),
    ('è', "e\u{0300}"),
    ('ê', "e\u{0302}"),
    ('ë', "e\u{0308}"),
    ('ì', "i\u{0300}"),
    ('í', "i\u{0301}"),
    ('î', "i\u{0302}"),
    ('ï', "i\u{0308}"),
    ('ñ', "n\u{0303}"),
    ('ò', "o\u{0300}"),
    ('ó', "o\u{0301}"),
    ('ô', "o\u{0302}"),
    ('õ', "o\u{0303}"),
    ('ö', "o\u{0308}"),
    ('ù', "u\u{0300}"),
    ('ú', "u\u{0301}"),
    ('û', "u\u{0302}"),
    ('ü', "u\u{0308}"),
    ('ý', "y\u{0301}"),
    ('ÿ', "y\u{0308}"),
];

/// The ligatures open up only in the loose pass — "œuvre" and
/// "oeuvre", "straße" and "strasse" are the same word, but the
/// learner should see the ligature (or the German sharp s) spelled
/// out.
const LIGATURES: [(char, &str); 3] = [('œ', "oe"), ('æ', "ae"), ('ß', "ss")];

/// Punctuation that may differ at the ends of an answer: a missing "?"
/// must not fail "How are you?", and Spanish's inverted marks must not
/// fail "¡Hola!" — "hola" is right (D9).
const EDGE_PUNCTUATION: [char; 7] = ['.', '?', '!', ',', ';', '¡', '¿'];

/// The normalization both sides go through before comparing: trim,
/// lowercase, collapse internal whitespace, strip leading and trailing
/// punctuation, fold apostrophes, and fold Unicode composition for the
/// accented Latin-1 letters. Accents are still significant here —
/// "etudiant" is NOT yet "étudiant".
pub fn normalize_strict(input: &str) -> String {
    let collapsed: String = input
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let stripped = strip_edge_punctuation(&collapsed);
    let mut out = String::with_capacity(stripped.len());
    for c in stripped.chars() {
        match c {
            '’' | '‘' => out.push('\''),
            _ => match ACCENTED.iter().find(|(letter, _)| *letter == c) {
                Some((_, expansion)) => out.push_str(expansion),
                None => out.push(c),
            },
        }
    }
    out
}

/// Strict normalization plus the accent fold: combining marks drop
/// ("étudiant" == "etudiant") and the ligatures open ("œuvre" ==
/// "oeuvre"). A loose-only match is correct with a hint.
pub fn normalize_loose(input: &str) -> String {
    let strict = normalize_strict(input);
    let mut out = String::with_capacity(strict.len());
    for c in strict.chars() {
        // combining diacritical marks
        if ('\u{0300}'..='\u{036F}').contains(&c) {
            continue;
        }
        match LIGATURES.iter().find(|(letter, _)| *letter == c) {
            Some((_, expansion)) => out.push_str(expansion),
            None => out.push(c),
        }
    }
    out
}

fn strip_edge_punctuation(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    while chars
        .first()
        .is_some_and(|c| c.is_whitespace() || EDGE_PUNCTUATION.contains(c))
    {
        chars.remove(0);
    }
    while chars
        .last()
        .is_some_and(|c| c.is_whitespace() || EDGE_PUNCTUATION.contains(c))
    {
        chars.pop();
    }
    chars.into_iter().collect()
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
                    accepted_answers: vec![],
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

    fn translation(answer: &str, accepted: &[&str]) -> Exercise {
        Exercise::Translation {
            prompt: "…".into(),
            answer: answer.into(),
            accepted_answers: accepted.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// One row per grading rule — the table the review asked for.
    #[test]
    fn grading_normalization_table() {
        let cases: Vec<(&str, Exercise, Grade)> = vec![
            // trim + case + internal whitespace
            ("  BONJOUR  ", translation("bonjour", &[]), Grade::Correct),
            (
                "comment   ça   va",
                translation("comment ça va", &[]),
                Grade::Correct,
            ),
            // leading/trailing punctuation is forgiven, not internal
            (
                "How are you?",
                translation("how are you", &[]),
                Grade::Correct,
            ),
            ("Bonjour !", translation("bonjour", &[]), Grade::Correct),
            ("a.b", translation("ab", &[]), Grade::Incorrect),
            // apostrophes fold
            ("l’homme", translation("l'homme", &[]), Grade::Correct),
            // accent fold: correct, with the accented form as the hint
            (
                "etudiant",
                translation("étudiant", &[]),
                Grade::CorrectWithHint("étudiant".into()),
            ),
            (
                "Je suis fatigue",
                translation("Je suis fatigué", &[]),
                Grade::CorrectWithHint("Je suis fatigué".into()),
            ),
            // composition fold: decomposed é equals precomposed é, no hint
            (
                "e\u{0301}tudiant",
                translation("étudiant", &[]),
                Grade::Correct,
            ),
            (
                "e\u{0301}tudiant",
                translation("etudiant", &[]),
                Grade::CorrectWithHint("etudiant".into()),
            ),
            // ligatures open, with a hint
            (
                "oeuvre",
                translation("œuvre", &[]),
                Grade::CorrectWithHint("œuvre".into()),
            ),
            // Spanish (D9): ñ and the acute vowels fold like any accent,
            // and the inverted ¡ ¿ are edge punctuation — "hola" is
            // "¡Hola!" without the decoration
            (
                "manana",
                translation("mañana", &[]),
                Grade::CorrectWithHint("mañana".into()),
            ),
            (
                "como estas",
                translation("¿Cómo estás?", &[]),
                Grade::CorrectWithHint("¿Cómo estás?".into()),
            ),
            ("hola", translation("¡Hola!", &[]), Grade::Correct),
            // German (D9): the sharp s opens like a ligature, with a hint
            (
                "strasse",
                translation("straße", &[]),
                Grade::CorrectWithHint("straße".into()),
            ),
            // Portuguese (D9): ã folds too — the table is the whole
            // Latin-1 set, not a French dialect
            (
                "irmao",
                translation("irmão", &[]),
                Grade::CorrectWithHint("irmão".into()),
            ),
            // but ø is a letter in its own right, not an accented o —
            // the table deliberately stops at accent+base pairs
            ("smor", translation("smør", &[]), Grade::Incorrect),
            // accepted alternative spellings
            (
                "I'm a student",
                translation("I am a student", &["I'm a student"]),
                Grade::Correct,
            ),
            (
                "je suis etudiante",
                translation("je suis étudiant", &["je suis étudiante"]),
                Grade::CorrectWithHint("je suis étudiante".into()),
            ),
            // and still wrong answers are wrong
            ("salut", translation("bonjour", &[]), Grade::Incorrect),
            ("", translation("bonjour", &[]), Grade::Incorrect),
        ];
        for (attempt, exercise, expected) in cases {
            assert_eq!(exercise.grade(attempt), expected, "attempt {attempt:?}");
        }
    }

    #[test]
    fn accent_only_mismatches_count_as_correct() {
        let exercise = translation("étudiant", &[]);
        assert!(exercise.is_correct("etudiant"));
        assert!(exercise.is_correct("ÉTUDIANT"));
        assert!(!exercise.is_correct("étudiante"));
    }

    #[test]
    fn the_new_latin_letters_behave_like_the_french_ones() {
        // The Spanish wave (D9) widened the table from 16 French
        // letters to the Latin-1 Supplement set — the newcomers get
        // the same three behaviors: fold with a hint, fold from
        // uppercase, and stay distinct from actually different words.
        let exercise = translation("mañana", &[]);
        assert_eq!(
            exercise.grade("manana"),
            Grade::CorrectWithHint("mañana".into())
        );
        assert!(exercise.is_correct("MAÑANA"));
        // decomposed input equals precomposed — no hint needed
        assert_eq!(exercise.grade("man\u{0303}ana"), Grade::Correct);
        assert!(!exercise.is_correct("mananaa"));
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
            accepted_answers: vec![],
        };
        assert_eq!(mc.prompt(), "pick one");
        assert_eq!(mc.answer(), "x");
        assert!(mc.accepted_answers().is_empty());
        assert_eq!(fb.prompt(), "___ means hello");
        assert_eq!(fb.answer(), "bonjour");
        assert!(fb.accepted_answers().is_empty());
    }
}
