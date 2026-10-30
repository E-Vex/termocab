//! CLI — git-style direct subcommands, hand-rolled from std::env::args().
//!
//! No CLI-parsing crate: with eight verbs and one optional flag, a match
//! over the argument list is clearer than a dependency (§6 of the
//! handoff). Verbs:
//!   termocab start <lang> [stage]   run the next (or a given) stage
//!   termocab review <lang>          spaced-repetition review of due words
//!   termocab list-words <lang>      all vocabulary in a language
//!   termocab progress <lang>        completed stages + streak + due count
//!   termocab streak <lang>          just the streak number
//!   termocab add-lang <lang> <file> install a stages.json as a language
//!                                 (add --force to replace an existing one)
//!   termocab check <file>           validate a stages.json without installing
//!   termocab help                   usage text
//!
//! Exit codes follow the Unix habits git users expect: 0 success, 1
//! runtime failure, 2 usage error (same as git). Errors are values
//! here — [`run`] returns `Result<(), CliError>` and `main` is the only
//! place that prints and exits — so the CLI is unit-testable and no
//! helper skips destructors or stdout flushes by calling `exit` mid-run
//! (review item 14).

use crate::content;
use crate::engine::Vocabulary;
use crate::lang::LangId;
use crate::progress::LanguageProgress;
use crate::session::{self, StdioTerminal};
use crate::storage;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
termocab — language learning for terminal enthusiasts

usage: termocab <command> [args]

commands:
  start <lang> [stage]     run the next uncompleted stage (or a specific one)
  review <lang>            review due vocabulary with spaced repetition
  list-words <lang>         list all vocabulary in a language's stages
  progress <lang>           show completed stages, streak and due reviews
  streak <lang>             print the current streak in days
  add-lang [--force] <lang> <file>  install a stages.json file as a new language
  check <file>              validate a stages.json file without installing it
  help                      show this help";

/// Everything that can end a termocab run early. Carries the exit code,
/// so `main` decides once how to report and exit — no `process::exit`
/// from deep inside a command helper.
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// Bad invocation: unknown command, wrong argument count, invalid
    /// language name. Exit code 2, like git.
    Usage(String),
    /// The invocation was fine but the run failed: unreadable progress,
    /// locked stage, invalid content, … Exit code 1.
    Runtime(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Runtime(_) => 1,
        }
    }

    /// The message `main` prints after the "termocab: " prefix. Usage
    /// errors also point at `termocab help`, like git does.
    pub fn message(&self) -> String {
        match self {
            CliError::Usage(msg) => format!("{msg}\nrun 'termocab help' for usage."),
            CliError::Runtime(msg) => msg.clone(),
        }
    }
}

/// Where language content lives. `$TERMOCAB_CONTENT_DIR` overrides the
/// default, which is `languages/` relative to the current directory —
/// the same place `cargo run` finds it.
fn content_dir() -> PathBuf {
    match std::env::var("TERMOCAB_CONTENT_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from("languages"),
    }
}

fn stages_path(lang: &LangId) -> PathBuf {
    content_dir().join(lang.as_str()).join("stages.json")
}

/// The config root for user state, resolved from the environment here —
/// the CLI edge — and handed to the storage layer, which never reads the
/// environment itself (item 16). Failing loudly here is item 10: never
/// fall back to writing in the current directory.
fn config_root() -> Result<PathBuf, CliError> {
    storage::config_root(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
    .map_err(|e| runtime(e.to_string()))
}

/// "Today" as YYYY-MM-DD. UTC-based unless the TZHOURS override shifts
/// it — see [`local_utc_offset_seconds`] for why the real local clock
/// is not read yet (review item 6 tracks the fix).
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + local_utc_offset_seconds();
    civil_from_unix(secs)
}

/// UTC offset in seconds for the local timezone, taken from the TZHOURS
/// override (hours, may be negative). std cannot read the system's
/// timezone — reading the TZ database (DST rules included) means either
/// a time crate or parsing /etc/localtime ourselves; the dependency rule
/// (§6) and review item 6 track that decision. Until then TZHOURS is the
/// explicit, honest override.
fn local_utc_offset_seconds() -> i64 {
    std::env::var("TZHOURS")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .map(|h| h * 3600)
        .unwrap_or(0)
}

/// Converts unix seconds (UTC) to a YYYY-MM-DD civil date.
/// Howard Hinnant's public-domain civil-from-days algorithm.
fn civil_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// What main() hands us: the raw argument list after the program name.
pub fn run(args: &[String]) -> Result<(), CliError> {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{USAGE}");
        return Ok(());
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => {
            expect_no_args(rest, "help")?;
            println!("{USAGE}");
            Ok(())
        }
        "start" => start(rest),
        "review" => review(rest),
        "list-words" => list_words(rest),
        "progress" => progress(rest),
        "streak" => streak(rest),
        "add-lang" => add_lang(rest),
        "check" => check(rest),
        other => Err(CliError::Usage(format!("unknown command: {other}"))),
    }
}

fn usage(msg: impl Into<String>) -> CliError {
    CliError::Usage(msg.into())
}

fn runtime(msg: impl Into<String>) -> CliError {
    CliError::Runtime(msg.into())
}

/// Commands that take no arguments at all.
fn expect_no_args(rest: &[String], cmd: &str) -> Result<(), CliError> {
    if !rest.is_empty() {
        return Err(usage(format!(
            "'{cmd}' takes no arguments\nusage: termocab {cmd}"
        )));
    }
    Ok(())
}

/// Commands that take exactly one argument (a language): parse it as a
/// [`LangId`] and reject anything else, naming the command's usage
/// line (item 9). The lang id is validated here, at the CLI edge, so a
/// traversal attempt like `../x` fails as a usage error (item 4).
fn expect_lang(rest: &[String], cmd: &str) -> Result<LangId, CliError> {
    let [lang] = rest else {
        return Err(usage(format!(
            "'{cmd}' takes exactly one argument: a language\nusage: termocab {cmd} <lang>"
        )));
    };
    parse_lang(lang)
}

/// Parses a language name into a [`LangId`] or reports a usage error.
fn parse_lang(raw: &str) -> Result<LangId, CliError> {
    LangId::parse(raw).map_err(|reason| usage(format!("invalid language name: {reason}")))
}

/// start <lang> [stage] — run the next uncompleted stage, or a chosen
/// one. Records progress on pass. There is no placement system yet, so
/// every learner starts at Stage 1 (documented in the handoff, §9).
/// Stages unlock strictly in order: an explicit stage number may only be
/// a completed stage (replay) or the next uncompleted one (review item 7
/// / decision D4 — the original spec's unlock condition, now enforced).
fn start(rest: &[String]) -> Result<(), CliError> {
    if rest.is_empty() || rest.len() > 2 {
        return Err(usage(
            "'start' takes a language and an optional stage number\nusage: termocab start <lang> [stage]",
        ));
    }
    let lang = parse_lang(&rest[0])?;
    let stages = load_lang(&lang)?;
    let root = config_root()?;

    let mut prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;

    let stage = match rest.get(1) {
        Some(n) => match n.parse::<u32>() {
            Ok(id) => {
                let stage = stages
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| runtime(format!("no stage {id} in {lang}")))?;
                let unlocked =
                    prog.is_stage_completed(id) || prog.current_stage_id(&stages) == Some(id);
                if !unlocked {
                    return match prog.current_stage_id(&stages) {
                        Some(next) => Err(runtime(format!(
                            "stage {id} is locked — complete stage {next} first"
                        ))),
                        None => Err(runtime(format!("stage {id} is locked"))),
                    };
                }
                stage
            }
            Err(_) => {
                return Err(usage(
                    "stage must be a number, e.g. 'termocab start french 1'",
                ))
            }
        },
        None => match prog.next_stage(&stages) {
            Some(s) => s,
            None => {
                println!("every stage in {lang} is complete. nothing left to start.");
                return Ok(());
            }
        },
    };

    let mut term = StdioTerminal;
    let today = today();
    // Snapshot what the run could change, so the file is written only
    // when something actually does: a failed or aborted run never saves,
    // and a passed replay on an already-recorded day rewrites nothing
    // (item 11 — the old code rewrote the file even on </dev/null runs).
    let stage_was_done = prog.is_stage_completed(stage.id);
    let day_was_recorded = prog.is_active_day(&today);
    let outcome = session::run_and_record(&mut term, stage, &mut prog, &today);
    let changed = outcome.passed() && !(stage_was_done && day_was_recorded);
    if changed {
        storage::save(&root, &lang, &prog).map_err(|e| runtime(e.to_string()))?;
        println!("progress saved.");
    }
    Ok(())
}

/// Vocabulary of every completed stage — the words eligible for
/// spaced-repetition review. Anything the learner hasn't met by
/// completing a stage is deliberately not scheduled yet, so reviews
/// only ever ask about material that was actually taught.
fn reviewable_words<'a>(
    stages: &'a [crate::engine::Stage],
    prog: &LanguageProgress,
) -> Vec<&'a Vocabulary> {
    stages
        .iter()
        .filter(|s| prog.is_stage_completed(s.id))
        .flat_map(|s| s.vocabulary.iter())
        .collect()
}

/// Synchronizes the srs state with the completed stages' vocabulary
/// (in memory — `review` persists it) and returns it, plus how many
/// cards are due on `today`.
fn due_reviews(
    root: &Path,
    lang: &LangId,
    stages: &[crate::engine::Stage],
    prog: &LanguageProgress,
    today: &str,
) -> Result<(crate::srs::SrsState, usize), CliError> {
    let mut srs = storage::load_srs(root, lang).map_err(|e| runtime(e.to_string()))?;
    let words: Vec<String> = reviewable_words(stages, prog)
        .into_iter()
        .map(|v| v.word.clone())
        .collect();
    srs.introduce(&words, today);
    let due = srs.due_on(today).len();
    Ok((srs, due))
}

/// review <lang> — spaced-repetition session over the vocabulary of
/// completed stages (Leitner boxes, see `srs/`). Any answered card
/// records the day in progress, so a review keeps the streak alive
/// exactly like a completed stage does. Nothing is written when there
/// was nothing to answer.
fn review(rest: &[String]) -> Result<(), CliError> {
    let lang = expect_lang(rest, "review")?;
    let stages = load_lang(&lang)?;
    let root = config_root()?;
    let mut prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    let today = today();
    let (mut srs, due) = due_reviews(&root, &lang, &stages, &prog, &today)?;

    if due == 0 {
        match srs.next_due(&today) {
            Some((date, n)) => {
                println!("no cards due today. next review: {date} ({n} word(s)).")
            }
            None => println!(
                "no cards due for {lang} yet — complete a stage to start scheduling reviews."
            ),
        }
        return Ok(());
    }

    let vocabulary = reviewable_words(&stages, &prog);
    let mut term = StdioTerminal;
    let outcome = session::run_review(&mut term, &mut srs, &vocabulary, &today);

    if outcome.answered > 0 {
        // Every answered card was rescheduled, so the srs state always
        // changes. The progress file only changes when today is a new
        // active day — otherwise it would be rewritten with identical
        // content on every review.
        let day_is_new = !prog.is_active_day(&today);
        prog.record_active_day(&today);
        storage::save_srs(&root, &lang, &srs).map_err(|e| runtime(e.to_string()))?;
        if day_is_new {
            storage::save(&root, &lang, &prog).map_err(|e| runtime(e.to_string()))?;
        }
        println!("review saved.");
    }
    Ok(())
}

/// list-words <lang> — every vocabulary entry across all stages.
fn list_words(rest: &[String]) -> Result<(), CliError> {
    let lang = expect_lang(rest, "list-words")?;
    let stages = load_lang(&lang)?;
    for stage in &stages {
        println!(
            "Stage {} — {} [{}]",
            stage.id, stage.title, stage.level_hint
        );
        for v in &stage.vocabulary {
            println!("  {} ({}) — {}", v.word, v.pos, v.translation);
            println!("    e.g. {}", v.example);
        }
    }
    Ok(())
}

/// progress <lang> — completed stages, current streak, due reviews.
fn progress(rest: &[String]) -> Result<(), CliError> {
    let lang = expect_lang(rest, "progress")?;
    let stages = load_lang(&lang)?;
    let root = config_root()?;
    let prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    let today = today();
    let done = prog.completed_stages();
    println!("{lang}: {}/{} stage(s) complete", done.len(), stages.len());
    for stage in &stages {
        let mark = if prog.is_stage_completed(stage.id) {
            "[x]"
        } else {
            "[ ]"
        };
        println!(
            "  {mark} Stage {} — {} [{}]",
            stage.id, stage.title, stage.level_hint
        );
    }
    match prog.current_stage_id(&stages) {
        Some(id) => println!("  next up: stage {id}"),
        None => println!("  all stages complete."),
    }
    println!("  streak: {} day(s)", prog.streak_on(&today));
    let (_, due) = due_reviews(&root, &lang, &stages, &prog, &today)?;
    println!("  reviews due today: {due} word(s) — run 'termocab review {lang}'");
    Ok(())
}

/// streak <lang> — print just the streak count, script-friendly.
fn streak(rest: &[String]) -> Result<(), CliError> {
    let lang = expect_lang(rest, "streak")?;
    let root = config_root()?;
    let prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    println!("{}", prog.streak_on(&today()));
    Ok(())
}

/// add-lang [--force] <lang> <file> — validate and copy a stages.json
/// into the content dir, so the next `start` can find it. An existing
/// language is never silently replaced: recorded stage completions and
/// srs cards may point at content that would change under them, so the
/// overwrite must be named explicitly with --force (item 12).
fn add_lang(rest: &[String]) -> Result<(), CliError> {
    let force = rest.iter().any(|arg| arg == "--force");
    let args: Vec<&String> = rest.iter().filter(|arg| *arg != "--force").collect();
    if args.len() != 2 {
        return Err(usage(
            "'add-lang' takes a language and a stages.json file, plus an optional --force\nusage: termocab add-lang [--force] <lang> <file>",
        ));
    }
    let lang = parse_lang(args[0])?;
    let src = Path::new(args[1]);

    // Validate before installing: it must parse as our content format and
    // pass every content rule, including declaring itself as `lang`.
    let loaded = content::load_language(src, Some(&lang))
        .map_err(|e| runtime(format!("not a valid stages file: {e}")))?;

    let dest = stages_path(&lang);
    let replacing = dest.exists();
    if replacing && !force {
        return Err(runtime(format!(
            "language '{lang}' already exists at {} — use --force to replace it",
            dest.display()
        )));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| runtime(format!("could not create content dir: {e}")))?;
    }
    std::fs::copy(src, &dest)
        .map_err(|e| runtime(format!("could not install language file: {e}")))?;
    let verb = if replacing { "replaced" } else { "added" };
    println!(
        "{verb} language '{lang}' — {} stage(s) installed at {}",
        loaded.stages.len(),
        dest.display()
    );
    Ok(())
}

fn load_lang(lang: &LangId) -> Result<Vec<crate::engine::Stage>, CliError> {
    let path = stages_path(lang);
    match content::load_language(&path, Some(lang)) {
        Ok(loaded) => Ok(loaded.stages),
        Err(content::LoadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(runtime(format!(
                "no content for language '{lang}'. add one with: termocab add-lang {lang} <stages.json>"
            )))
        }
        Err(e) => Err(runtime(format!("{lang}: {e}"))),
    }
}

/// check <file> — run the content validator on a stages.json file
/// without installing it, so a content author gets the full violation
/// list (and a quick sanity summary when the file is good) before
/// offering it to a learner.
fn check(rest: &[String]) -> Result<(), CliError> {
    let [file] = rest else {
        return Err(usage(
            "'check' takes exactly one argument: a stages.json file\nusage: termocab check <file>",
        ));
    };
    match content::load_language(Path::new(file), None) {
        Ok(loaded) => {
            let exercises: usize = loaded.stages.iter().map(|s| s.exercises.len()).sum();
            println!(
                "ok: {} stage(s), {exercises} exercise(s), language '{}'",
                loaded.stages.len(),
                loaded.language
            );
            Ok(())
        }
        Err(e) => Err(runtime(format!("{file}: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_matches_known_values() {
        assert_eq!(civil_from_unix(0), "1970-01-01");
        assert_eq!(civil_from_unix(1_755_846_000), "2025-08-22"); // 2025-08-22 05:00 UTC
        assert_eq!(civil_from_unix(951_782_400), "2000-02-29"); // leap day
    }

    // With run() returning Result (item 14), CLI behaviour is testable
    // without a process — these are the first of the item-15 batch.

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn unknown_command_is_a_usage_error() {
        let err = run(&args(&["frobnicate"])).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.message().contains("unknown command: frobnicate"));
    }

    #[test]
    fn help_with_extra_arguments_is_a_usage_error() {
        let err = run(&args(&["help", "please"])).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.message().contains("'help' takes no arguments"));
    }

    #[test]
    fn arity_errors_name_the_usage_line() {
        for (cmd_line, fragment) in [
            (
                vec!["progress", "french", "junk"],
                "termocab progress <lang>",
            ),
            (vec!["streak", "french", "junk"], "termocab streak <lang>"),
            (vec!["review", "french", "junk"], "termocab review <lang>"),
            (
                vec!["list-words", "french", "junk"],
                "termocab list-words <lang>",
            ),
            (
                vec!["add-lang", "french"],
                "termocab add-lang [--force] <lang> <file>",
            ),
            (vec!["check", "a.json", "b.json"], "termocab check <file>"),
            (vec!["start"], "termocab start <lang> [stage]"),
        ] {
            let err = run(&args(&cmd_line)).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{cmd_line:?}");
            assert!(err.message().contains(fragment), "{}", err.message());
        }
    }

    #[test]
    fn invalid_language_names_are_usage_errors() {
        for bad in ["..", "../x", "a/b", "FRENCH", "café"] {
            let err = run(&args(&["progress", bad])).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{bad:?}");
            assert!(err.message().contains("invalid language name"), "{bad:?}");
        }
    }

    #[test]
    fn cli_config_root_reads_the_environment() {
        // The one env-reading test so far — holds the shared env lock so
        // it never races another env-touching test in the same process.
        let _env = crate::testenv::env_lock();
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/termocab-cfg-test");
        assert_eq!(
            config_root().unwrap(),
            PathBuf::from("/tmp/termocab-cfg-test/termocab")
        );
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::set_var("HOME", "/tmp/termocab-home-test");
        assert_eq!(
            config_root().unwrap(),
            PathBuf::from("/tmp/termocab-home-test/.config/termocab")
        );
        std::env::remove_var("HOME");
    }

    /// A stages.json body for add-lang tests: one valid stage.
    const VALID_STAGES: &str = r#"{
        "language": "french",
        "stages": [
          { "id": 1, "title": "One", "level_hint": "beginner",
            "vocabulary": [
              { "word": "bonjour", "translation": "hello", "pos": "x", "example": "Bonjour !" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "bonjour", "answer": "hello" }
            ] }
        ]
      }"#;

    #[test]
    fn add_lang_installs_refuses_overwrite_and_validates() {
        let _env = crate::testenv::env_lock();
        let content = crate::testenv::TempDir::new("addlang-content");
        std::env::set_var("TERMOCAB_CONTENT_DIR", content.path());

        let src = crate::testenv::TempDir::new("addlang-src");
        let valid = src.path().join("stages.json");
        std::fs::write(&valid, VALID_STAGES).unwrap();
        let valid = valid.to_str().unwrap().to_string();

        // happy path: installs under <content dir>/<lang>/stages.json
        run(&args(&["add-lang", "french", &valid])).unwrap();
        assert!(content.path().join("french/stages.json").exists());

        // a second add of the same language is refused without --force
        let err = run(&args(&["add-lang", "french", &valid])).unwrap_err();
        assert_eq!(err.exit_code(), 1);
        assert!(err.message().contains("use --force to replace it"));

        // --force replaces it
        run(&args(&["add-lang", "--force", "french", &valid])).unwrap();

        // invalid content is rejected, every violation listed, nothing installed
        let bad = src.path().join("bad.json");
        std::fs::write(
            &bad,
            r#"{ "language": "german", "stages": [
                 { "id": 1, "title": "X", "level_hint": "b",
                   "vocabulary": [],
                   "exercises": [
                     { "type": "multiple_choice", "prompt": "p",
                       "options": ["x", "y"], "answer": "z" }
                   ] }
               ] }"#,
        )
        .unwrap();
        let bad = bad.to_str().unwrap().to_string();
        let err = run(&args(&["add-lang", "german", &bad])).unwrap_err();
        assert_eq!(err.exit_code(), 1);
        assert!(err
            .message()
            .contains("answer \"z\" is not one of the options"));
        assert!(!content.path().join("german").exists());

        // and the installed language actually loads through `progress`
        // (the config dir is irrelevant here; content is all add-lang needs)
        let cfg = crate::testenv::TempDir::new("addlang-cfg");
        std::env::set_var("XDG_CONFIG_HOME", cfg.path());
        run(&args(&["progress", "french"])).unwrap();
    }

    fn stage_with_vocab(id: u32, words: &[&str]) -> crate::engine::Stage {
        crate::engine::Stage {
            id,
            title: format!("Stage {id}"),
            level_hint: "beginner".into(),
            vocabulary: words
                .iter()
                .map(|w| crate::engine::Vocabulary {
                    word: w.to_string(),
                    translation: format!("translation of {w}"),
                    pos: "noun".into(),
                    example: format!("{w} example"),
                })
                .collect(),
            exercises: vec![],
        }
    }

    #[test]
    fn reviewable_words_come_from_completed_stages_only() {
        let stages = vec![
            stage_with_vocab(1, &["bonjour", "merci"]),
            stage_with_vocab(2, &["suis"]),
        ];
        let mut p = LanguageProgress::new();
        p.record_stage_completion(1, "2026-10-30");
        let words: Vec<&str> = reviewable_words(&stages, &p)
            .into_iter()
            .map(|v| v.word.as_str())
            .collect();
        assert_eq!(words, vec!["bonjour", "merci"]);
    }
}
