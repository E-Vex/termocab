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
/// European set (French, Spanish, German, Italian, Portuguese, …) —
/// plus the decomposable Polish set from Latin Extended-A (D10), the
/// Romanian set (D11) — ă (a + breve, Extended-A) and ș ț (s/t +
/// comma below, Extended-B) — and the Hungarian pair ő ű (D14), both
/// Extended-A, both decomposing to base letter + the double acute.
/// The legacy cedilla spellings ş ţ ride along (s/t + cedilla — the
/// same mark ç already carries), so both historical encodings grade
/// the same way — and that same cedilla entry is also how Turkish's
/// own ş rides for free (D15). The Turkish soft ğ joins the same
/// way (D15): g + breve, the very mark Romanian's ă carries, so
/// "soguk" counts with "soğuk" as the hint. Dotless ı is the other
/// half of the Turkish question and goes in NO table: ı and i are
/// two different vowels, not one word spelled twice, so ı joins
/// ø þ ð in the closed club (D9) — "hayır" is not "hayir" — and
/// capital İ needs no entry either: to_lowercase leaves "i" +
/// U+0307 behind and the loose pass drops the dot, so "istanbul"
/// counts with "İstanbul" through the existing machinery.
/// Decomposing (rather than stripping) in the strict
/// pass makes precomposed "é" equal to "e" + U+0301 typed on a macOS
/// keyboard — same letter, same accent, different encoding. Hand-
/// written on purpose: forty-two entries beat a Unicode-normalization
/// crate, and §6 asks for justification before any dependency. The
/// Nordic letters stay closed — "smør" is not "smor". Lowercase
/// only: [`normalize_strict`] lowercases before it looks here (D9).
const ACCENTED: [(char, &str); 42] = [
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
    // Latin Extended-A, the Polish set (D10): ogonek, acute and
    // dot above are combining marks like any other, so the loose
    // pass drops them with no further change.
    ('ą', "a\u{0328}"),
    ('ć', "c\u{0301}"),
    ('ę', "e\u{0328}"),
    ('ń', "n\u{0301}"),
    ('ś', "s\u{0301}"),
    ('ź', "z\u{0301}"),
    ('ż', "z\u{0307}"),
    // Romanian (D11): the breve and — contrary to the D10 prediction
    // — the comma below turn out to be ordinary combining marks (ș ț
    // decompose after all, Extended-B included), so the whole set
    // lands here instead of [`OPEN_LETTERS`]; the cedilla pair ş ţ
    // joins for the legacy spellings of the same two letters.
    ('ă', "a\u{0306}"),
    ('ș', "s\u{0326}"),
    ('ț', "t\u{0326}"),
    ('ş', "s\u{0327}"),
    ('ţ', "t\u{0327}"),
    // Hungarian (D14): the double acute — two strokes standing on one
    // letter, the widest accent in the table — decomposes like any
    // other, so ő and ű (both Extended-A) land beside the Polish and
    // Romanian sets rather than in OPEN_LETTERS. The third extension
    // a language has forced, and the smallest: two entries, exactly
    // the D10-shaped addition the roadmap predicted. ő is also the
    // first table letter that is a word in its own right — "he/she".
    ('ő', "o\u{030B}"),
    ('ű', "u\u{030B}"),
    // Turkish (D15): the soft g — ğ, "yumuşak ge", which lengthens
    // the vowel before it and (mostly) never sounds on its own —
    // decomposes like any accented letter: g + breve U+0306, the
    // very mark Romanian's ă carries (D11), so it joins ACCENTED
    // beside the Polish, Romanian and Hungarian sets. The fourth
    // extension a language has forced. The other half of the
    // Turkish question goes the opposite way, in no table at all:
    // dotless ı has no decomposition and does not open — ı and i are
    // different vowels in their own right, exactly what ø þ ð are to
    // their neighbors — so "hayır" is not "hayir", and D9's closed
    // club takes a fourth member. Capital İ needs no entry either:
    // to_lowercase turns it into "i" + U+0307 and the loose pass
    // drops the dot, so "istanbul" counts with "İstanbul" as the
    // hint through machinery that already exists.
    ('ğ', "g\u{0306}"),
];

/// The ligatures open up only in the loose pass — "œuvre" and
/// "oeuvre", "straße" and "strasse" are the same word, but the
/// learner should see the ligature (or the German sharp s) spelled
/// out. Dutch's ĳ (D19) opens to the two letters it spells, the way
/// œ opens to oe and æ to ae: modern Dutch types the two-letter ij
/// everywhere, so a learner answering "IJsselmeer" for "Ĳsselmeer"
/// counts, with the ligature form as the hint. The y of the old
/// schoolbook shorthand ("myn" for "mĳn") is no spelling of the word
/// and stays a miss.
const LIGATURES: [(char, &str); 4] = [('œ', "oe"), ('æ', "ae"), ('ß', "ss"), ('ĳ', "ij")];

/// Single letters that open to a plain ASCII letter in the loose
/// pass only, exactly like the ligatures above: they have no Unicode
/// decomposition — they are letters in their own right — but the fold
/// is a spelling convention of the same word, not two different
/// words. Polish ł is the first entry (D10): a learner without a
/// Polish keyboard types "lodz" for "łódź" the same way "strasse"
/// stands in for "straße", so it counts, with the properly spelled
/// form as the hint. ø, þ and ð stay closed (D9): "smør" is not
/// "smor" — and Turkish dotless ı joins the closed club (D15): ı
/// and i are different vowels in their own right, so "hayır" is
/// not "hayir"; the roadmap's reopened letters-in-their-own-right
/// question, answered the D9 way. Romanian ș and ț were predicted
/// to join here (D10) but decompose after all — see [`ACCENTED`]
/// (D11); the table still holds just ł.
const OPEN_LETTERS: [(char, &str); 1] = [('ł', "l")];

/// Marks that fold away in the loose pass without being combining
/// diacritics. Catalan's interpunct (D18) — the "punt volat" that
/// separates the two l's of "col·legi" from the digraph "ll" — is
/// the first, and it answers the question the roadmap held open for
/// the twelfth language: is the raised dot decoration like ¡ ¿, or
/// meaningful like a letter? Neither. Not decoration, because ¡ and
/// ¿ only ever wrap an utterance and carry nothing inside the word,
/// while the interpunct sits inside the word and its absence is a
/// genuine misspelling — "collegi" is wrong in Catalan — so
/// stripping it in the strict pass would hand a full match to a
/// misspelled answer. Not a letter in its own right, because it has
/// no sound and no place in the alphabet, unlike the closed club
/// ø þ ð ı (D9, D15) whose members are phonemes with their own
/// slot. So it takes the tier ł built (D10): the same word missing
/// its mark counts, with the properly spelled form as the hint —
/// "collegi" counts with "col·legi" exactly like "lodz" counts
/// with "łódź". The ASCII hyphen some typists write instead
/// ("col-legi") stays a miss — folding internal hyphens would touch
/// every course that ships one, and the change is deliberately
/// additive only (the D14 rule). ŀ (U+0140, l with middle dot)
/// decomposes to l + · in Unicode but lives in no modern text —
/// the Portuguese â rule, one tier stricter — so it stays off the
/// ACCENTED table.
const FOLDED_MARKS: [char; 1] = ['\u{00B7}'];

/// Punctuation that may differ at the ends of an answer: a missing "?"
/// must not fail "How are you?", and Spanish's inverted marks must not
/// fail "¡Hola!" — "hola" is right (D9).
const EDGE_PUNCTUATION: [char; 7] = ['.', '?', '!', ',', ';', '¡', '¿'];

/// The normalization both sides go through before comparing: trim,
/// lowercase, collapse internal whitespace, strip leading and trailing
/// punctuation, fold apostrophes, and fold Unicode composition for the
/// accented Latin-1, Polish (Extended-A) and Romanian letters. Accents
/// are still significant here — "etudiant" is NOT yet "étudiant".
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
/// ("étudiant" == "etudiant"), the ligatures and open letters
/// unfold ("œuvre" == "oeuvre", "łódź" == "lodz" — D10), and the
/// folded marks vanish ("col·legi" == "collegi" — D18). A loose-only
/// match is correct with a hint.
pub fn normalize_loose(input: &str) -> String {
    let strict = normalize_strict(input);
    let mut out = String::with_capacity(strict.len());
    for c in strict.chars() {
        // combining diacritical marks drop, and so do the folded
        // marks beside them (D18)
        if ('\u{0300}'..='\u{036F}').contains(&c) || FOLDED_MARKS.contains(&c) {
            continue;
        }
        match LIGATURES
            .iter()
            .chain(OPEN_LETTERS.iter())
            .find(|(letter, _)| *letter == c)
        {
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
            // Polish (D10): the Latin Extended-A set folds exactly like
            // the Latin-1 one — ogonek, acute and dot above are just
            // combining marks — and ł, with no decomposition at all,
            // opens in the loose pass like ß
            (
                "dziekuje",
                translation("dziękuję", &[]),
                Grade::CorrectWithHint("dziękuję".into()),
            ),
            (
                "zolty",
                translation("żółty", &[]),
                Grade::CorrectWithHint("żółty".into()),
            ),
            (
                "lodz",
                translation("łódź", &[]),
                Grade::CorrectWithHint("łódź".into()),
            ),
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
    fn the_polish_extended_a_letters_behave_like_the_latin_1_ones() {
        // The fifth-language wave (D10) extended the table past the
        // Latin-1 Supplement into Extended-A: the decomposable Polish
        // set (ą ć ę ń ś ź ż — ogonek, acute, dot above) joined
        // ACCENTED, and ł, which has no decomposition, opens loose
        // exactly like ß. The newcomers get the same three behaviors:
        // fold with a hint, fold from uppercase, and stay distinct
        // from actually different words.
        let exercise = translation("żółty", &[]);
        assert_eq!(
            exercise.grade("zolty"),
            Grade::CorrectWithHint("żółty".into())
        );
        // all three diacritics in one word: ż, ó and ł
        assert!(exercise.is_correct("ŻÓŁTY"));
        assert!(!exercise.is_correct("zoltyy"));
        // decomposed input equals precomposed — no hint needed
        let dzien = translation("dzień", &[]);
        assert_eq!(dzien.grade("dzien\u{0301}"), Grade::Correct);
        // the stroke opens loose only — the ß rule, not the ø rule
        let lodz = translation("łódź", &[]);
        assert_eq!(lodz.grade("lodz"), Grade::CorrectWithHint("łódź".into()));
        assert!(lodz.is_correct("ŁÓDŹ"));
        assert!(!lodz.is_correct("ldz"));
        // ź and ż are distinct letters in strict but both fold to
        // plain z in loose — and the fold never invents a match with
        // an actually different word
        let zle = translation("źle", &[]);
        assert_eq!(zle.grade("zle"), Grade::CorrectWithHint("źle".into()));
        assert_eq!(zle.grade("złe"), Grade::CorrectWithHint("źle".into()));
        assert!(!zle.is_correct("dobrze"));
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
    fn the_romanian_letters_behave_like_the_polish_ones() {
        // The sixth-language wave (D11) extended the table again: ă
        // (a + breve) and — contrary to the D10 prediction — ș and ț
        // (s/t + comma below: they decompose after all, Extended-B
        // included) joined ACCENTED next to the Polish set, with the
        // legacy cedilla spellings ş ţ riding along. The newcomers
        // get the same three behaviors: fold with a hint, fold from
        // uppercase, and stay distinct from actually different words.
        let exercise = translation("mănâncă", &[]);
        assert_eq!(
            exercise.grade("mananca"),
            Grade::CorrectWithHint("mănâncă".into())
        );
        // ă, â and ă again in one word — and from uppercase
        assert!(exercise.is_correct("MĂNÂNCĂ"));
        assert!(!exercise.is_correct("manancaa"));
        // decomposed input equals precomposed — no hint needed
        // (the mark rides right after its base letter, exactly how
        // "e" + U+0301 equals "é")
        let sarpe = translation("șarpe", &[]);
        assert_eq!(sarpe.grade("s\u{0326}arpe"), Grade::Correct);
        // the comma below folds like any combining mark…
        assert_eq!(sarpe.grade("sarpe"), Grade::CorrectWithHint("șarpe".into()));
        // …and the legacy cedilla spelling folds to the same word,
        // with a hint — ş and ș are two encodings of one letter
        assert_eq!(sarpe.grade("şarpe"), Grade::CorrectWithHint("șarpe".into()));
        // ț: same story, comma below and legacy cedilla alike
        let tara = translation("țară", &[]);
        assert_eq!(tara.grade("tara"), Grade::CorrectWithHint("țară".into()));
        assert_eq!(tara.grade("ţară"), Grade::CorrectWithHint("țară".into()));
        assert!(tara.is_correct("ȚARĂ"));
        assert!(!tara.is_correct("carte"));
    }

    #[test]
    fn the_hungarian_letters_behave_like_the_polish_ones() {
        // The ninth-language wave (D14) extended the table a third
        // time, and by exactly the predicted amount: ő and ű (o/u +
        // double acute, U+030B — Extended-A) decompose like any
        // accented letter, so they join ACCENTED beside the Polish and
        // Romanian sets; OPEN_LETTERS still holds just ł. The
        // newcomers get the same three behaviors: fold with a hint,
        // fold from uppercase, and stay distinct from actually
        // different words.
        let exercise = translation("könnyű", &[]);
        assert_eq!(
            exercise.grade("konnyu"),
            Grade::CorrectWithHint("könnyű".into())
        );
        // both double acutes in one word — and from uppercase
        assert!(exercise.is_correct("KÖNNYŰ"));
        assert!(!exercise.is_correct("konnyuu"));
        // ő: same story — and the first table letter that is a word in
        // its own right (he/she), so the fold must not reach past it
        let no = translation("nő", &[]);
        assert_eq!(no.grade("no"), Grade::CorrectWithHint("nő".into()));
        assert!(no.is_correct("NŐ"));
        // "nők" is a different word (the plural) — the fold never
        // invents a match with it
        assert!(!no.is_correct("nok"));
        // decomposed input equals precomposed — no hint needed (the
        // double acute rides right after its base letter, exactly how
        // "e" + U+0301 equals "é")
        let ketto = translation("kettő", &[]);
        assert_eq!(ketto.grade("ketto\u{030B}"), Grade::Correct);
        // the double acute folds like any combining mark…
        assert_eq!(ketto.grade("ketto"), Grade::CorrectWithHint("kettő".into()));
        // …and ő ö stay distinct letters: fű (grass) is neither fő
        // (main) nor fo — the fold never crosses vowels
        let fur = translation("fű", &[]);
        assert_eq!(fur.grade("fu"), Grade::CorrectWithHint("fű".into()));
        assert!(!fur.is_correct("fő"));
        assert!(!fur.is_correct("fo"));
        assert!(!fur.is_correct("kő"));
    }

    #[test]
    fn the_turkish_letters_behave_like_the_polish_ones() {
        // The tenth-language wave (D15) split its question in two, and
        // each half got the answer the ø rule predicts. The soft ğ
        // (g + breve U+0306 — the very mark Romanian's ă carries,
        // D11) decomposes like any accented letter, so it joins
        // ACCENTED: "soguk" counts with "soğuk" as the hint, and
        // hand-decomposed equals precomposed with no hint at all.
        let exercise = translation("soğuk", &[]);
        assert_eq!(
            exercise.grade("soguk"),
            Grade::CorrectWithHint("soğuk".into())
        );
        // ğ from uppercase, one per word — and no invented matches
        assert!(exercise.is_correct("SOĞUK"));
        assert!(!exercise.is_correct("sogukk"));
        // decomposed input equals precomposed — no hint needed (the
        // breve rides right after its base letter, exactly how "a" +
        // U+0306 equals "ă")
        assert_eq!(exercise.grade("sog\u{0306}uk"), Grade::Correct);
        // ş rides the D11 legacy entry — the cedilla spelling Romanian
        // kept for its history is Turkish's own letter, so "kardes"
        // counts with "kardeş" with no new entry anywhere
        let kardes = translation("kardeş", &[]);
        assert_eq!(
            kardes.grade("kardes"),
            Grade::CorrectWithHint("kardeş".into())
        );
        assert!(kardes.is_correct("KARDEŞ"));
        // ı stays closed — the other half of the question: ı and i
        // are different vowels in their own right, exactly what ø is
        // to o, so the stripped spelling is wrong, not hinted
        let hayir = translation("hayır", &[]);
        assert_eq!(hayir.grade("hayir"), Grade::Incorrect);
        assert_eq!(hayir.grade("hayır"), Grade::Correct);
        assert!(!hayir.is_correct("hayirr"));
        // …and the one asymmetry the Unicode default case mapping
        // leaves: capital I lowercases to dotted i, so HAYIR (typed
        // with an ASCII capital) does not round-trip to hayır — the
        // classic Turkish-I problem. The engine is language-agnostic
        // (normalize_strict does not know which language it holds),
        // so D15 documents the edge instead of special-casing it.
        assert!(!hayir.is_correct("HAYIR"));
        // capital İ lowercases to "i" + U+0307, and the loose pass
        // drops the dot — so the plain spelling counts with the hint,
        // through machinery that needed no new entry
        let istanbul = translation("İstanbul", &[]);
        assert_eq!(
            istanbul.grade("istanbul"),
            Grade::CorrectWithHint("İstanbul".into())
        );
        assert!(istanbul.is_correct("İSTANBUL"));
        assert!(!istanbul.is_correct("ankara"));
    }

    #[test]
    fn the_icelandic_letters_were_all_on_file_from_d9() {
        // The eleventh-language wave (D17) is the payoff the D9 table
        // was holding: Icelandic needed no entry, no table, no rule —
        // its whole alphabet was on file before the course arrived.
        // þ and ð take the closed-club answer ø set and D15's ı
        // reaffirmed: letters in their own right, never hinted —
        // "thu" is wrong for "þú", "godur" is wrong for "góður" —
        // while the letters around them fold exactly like anywhere
        // else: "þu" counts with "þú" as the hint (ú folds, þ
        // stays), "goður" counts with "góður" (ó folds, ð stays).
        let thu = translation("þú", &[]);
        assert_eq!(thu.grade("þu"), Grade::CorrectWithHint("þú".into()));
        assert_eq!(thu.grade("thu"), Grade::Incorrect);
        assert!(thu.is_correct("ÞÚ"));
        let godur = translation("góður", &[]);
        assert_eq!(godur.grade("goður"), Grade::CorrectWithHint("góður".into()));
        assert_eq!(godur.grade("godur"), Grade::Incorrect);
        assert!(godur.is_correct("GÓÐUR"));
        // æ rides LIGATURES beside œ and ß — "hae" counts with "hæ"
        // as the hint, the fold the table shipped with in D9's day
        let hae = translation("hæ", &[]);
        assert_eq!(hae.grade("hae"), Grade::CorrectWithHint("hæ".into()));
        assert!(hae.is_correct("HÆ"));
        // and the acutes all decompose like any Latin-1 accent: ý on
        // "nýr", á on "já", ö on "tölva" — no new entries anywhere
        let nyr = translation("nýr", &[]);
        assert_eq!(nyr.grade("nyr"), Grade::CorrectWithHint("nýr".into()));
        let ja = translation("já", &[]);
        assert_eq!(ja.grade("ja"), Grade::CorrectWithHint("já".into()));
        let tolva = translation("tölva", &[]);
        assert_eq!(tolva.grade("tolva"), Grade::CorrectWithHint("tölva".into()));
        // að — the particle every Icelandic infinitive carries — is
        // plain ASCII through and through, and the closed letters
        // never cross vowels: "þyða" is not "þyda" (the ð), and
        // neither is it "þýda" spelled twice
        let thyda = translation("þýða", &[]);
        assert_eq!(thyda.grade("þyða"), Grade::CorrectWithHint("þýða".into()));
        assert_eq!(thyda.grade("þyda"), Grade::Incorrect);
        assert!(!thyda.is_correct("þyrr"));
    }

    #[test]
    fn the_catalan_interpunct_folds_like_the_stroke() {
        // The twelfth-language wave (D18) answers the question the
        // roadmap held open for Catalan: is the raised dot decoration
        // like ¡ ¿, or meaningful like a letter? Neither — it is an
        // internal mark the way ł's stroke is internal, so it takes
        // the open-letter tier: a correctly typed "col·legi" is a
        // full match (the dot survives the strict pass), and the
        // dotless "collegi" counts with the properly spelled form as
        // the hint, exactly like "lodz" counts with "łódź".
        let colegi = translation("col·legi", &[]);
        assert_eq!(colegi.grade("col·legi"), Grade::Correct);
        assert_eq!(
            colegi.grade("collegi"),
            Grade::CorrectWithHint("col·legi".into())
        );
        assert!(colegi.is_correct("Col·legi"));
        // the ASCII hyphen some typists fall back to stays a miss:
        // only the dot folds, never the hyphen — folding internal
        // hyphens would touch every course that ships one, and the
        // change is deliberately additive only (the D14 rule)
        assert_eq!(colegi.grade("col-legi"), Grade::Incorrect);
        // around the dot, everything folds as usual: "il·lusió"
        // carries the ó as well, so "il·lusio" hints the accent,
        // "illusio" hints accent and dot both, and the properly
        // spelled word is a full match
        let ilusio = translation("il·lusió", &[]);
        assert_eq!(ilusio.grade("il·lusió"), Grade::Correct);
        assert_eq!(
            ilusio.grade("il·lusio"),
            Grade::CorrectWithHint("il·lusió".into())
        );
        assert_eq!(
            ilusio.grade("illusio"),
            Grade::CorrectWithHint("il·lusió".into())
        );
        // and the accents of the rest of the course ride the Latin-1
        // table D9 shipped with Catalan's name already on it: à è é
        // í ï ó ò ú ç all decompose like any French accent — "gracies"
        // counts with "gràcies", "felic" with "feliç", "raim" with
        // "raïm" — which is why the course needed no other entry
        let gracies = translation("gràcies", &[]);
        assert_eq!(
            gracies.grade("gracies"),
            Grade::CorrectWithHint("gràcies".into())
        );
        let felic = translation("feliç", &[]);
        assert_eq!(felic.grade("felic"), Grade::CorrectWithHint("feliç".into()));
        let raim = translation("raïm", &[]);
        assert_eq!(raim.grade("raim"), Grade::CorrectWithHint("raïm".into()));
    }

    #[test]
    fn the_dutch_ligature_opens_to_its_two_letters() {
        // The thirteenth-language wave (D19) answers the question the
        // roadmap held open for Dutch: where does ĳ live, LIGATURES or
        // OPEN_LETTERS? With the ligatures — it is one, the name says
        // so, and its opening is the two-letter sequence it spells
        // ("ij"), exactly the œ → oe and æ → ae shape; OPEN_LETTERS
        // holds letters in their own right opening to a single letter
        // (ł → l), which ĳ is not. Modern Dutch types the two-letter
        // ij everywhere, so the two spellings meet in the loose pass:
        // a correctly ligatured "ĳsselmeer" is a full match (the
        // strict pass leaves it alone), the two-letter typing counts
        // with the ligature form as the hint, and the capital Ĳ
        // round-trips through to_lowercase with no D15-style
        // asymmetry — no authoring rule needed.
        let ijssel = translation("Ĳsselmeer", &[]);
        assert_eq!(ijssel.grade("ĳsselmeer"), Grade::Correct);
        assert_eq!(
            ijssel.grade("ijsselmeer"),
            Grade::CorrectWithHint("Ĳsselmeer".into())
        );
        // the capital ligature survives the default case mapping, so
        // an all-caps answer grades strict with no hint at all
        assert!(ijssel.is_correct("ĲSSELMEER"));
        assert!(!ijssel.is_correct("ijsselmere"));
        // in a running word, the ligature folds to the two letters it
        // spells — the old-orthography spelling grades like any
        // ligature word, and the y of the schoolbook shorthand is no
        // spelling at all: the fold is exactly i+j, never y
        let mijn = translation("mĳn", &[]);
        assert_eq!(mijn.grade("mĳn"), Grade::Correct);
        assert_eq!(mijn.grade("mijn"), Grade::CorrectWithHint("mĳn".into()));
        assert_eq!(mijn.grade("myn"), Grade::Incorrect);
        assert!(mijn.is_correct("MĲN"));
        assert!(!mijn.is_correct("mjn"));
        assert!(!mijn.is_correct("mijnn"));
        // the two-letter spelling typed with the ligature folds back
        // the same way, and the closed club ø þ ð ı gains nothing (D9,
        // D15): no Dutch letter is a letter in its own right the way
        // those are
        let tijd = translation("tijd", &[]);
        assert_eq!(tijd.grade("tĳd"), Grade::CorrectWithHint("tijd".into()));
        assert_eq!(tijd.grade("tijd"), Grade::Correct);
        // and the rest of the course rides the D9 table untouched: the
        // é of één folds like any accent, so "een" counts with "één"
        // as the hint — the one word where the accent is doing
        // grammar, separating the number from the article
        let een = translation("één", &[]);
        assert_eq!(een.grade("een"), Grade::CorrectWithHint("één".into()));
        assert!(een.is_correct("ÉÉN"));
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
