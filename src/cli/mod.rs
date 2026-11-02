//! CLI — git-style direct subcommands, hand-rolled from std::env::args().
//!
//! No CLI-parsing crate: with nine verbs and two flags, a match
//! over the argument list is clearer than a dependency (§6 of the
//! handoff). Verbs:
//!   termocab start [--no-shuffle|--seed <n>] <lang> [stage]
//!                                 run the next (or a given) stage —
//!                                 exercises shuffle by default (D7)
//!   termocab review <lang>          spaced-repetition review of due words
//!   termocab list-words <lang>      all vocabulary in a language
//!   termocab list-langs             every available language and where
//!                                 each one comes from (D16)
//!   termocab progress <lang>        completed stages + streak + due count
//!   termocab streak <lang>          just the streak number
//!   termocab stats <lang>           the learner's numbers + sparklines
//!   termocab add-lang <lang> <file> install a stages.json as a language
//!                                 (add --force to replace an existing one)
//!   termocab check <file>           validate a stages.json without installing
//!   termocab help                   usage text
//!
//! Exit codes follow the Unix habits git users expect: 0 success, 1
//! runtime failure, 2 usage error (same as git). A closed stdout
//! (broken pipe) exits 141 — silently, like cat and git (D6). Errors
//! are values here — [`run`] returns `Result<(), CliError>` and `main`
//! is the only place that prints and exits — so the CLI is
//! unit-testable and no helper skips destructors or stdout flushes by
//! calling `exit` mid-run (review item 14).

mod time;

use crate::content;
use crate::engine::Vocabulary;
use crate::lang::LangId;
use crate::progress::LanguageProgress;
use crate::session::{self, Color, ExerciseOrder, RunOptions, StdioTerminal};
use crate::srs;
use crate::storage;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "\
termocab — language learning for terminal enthusiasts

usage: termocab <command> [args]

commands:
  start [--no-shuffle|--seed <n>] <lang> [stage]
                            run the next uncompleted stage (or a specific one);
                            exercises shuffle by default
  review <lang>            review due vocabulary with spaced repetition
  list-words <lang>         list all vocabulary in a language's stages
  list-langs                list available languages and where each comes from
  progress <lang>           show completed stages, streak and due reviews
  streak <lang>             print the current streak in days
  stats <lang>              words, Leitner boxes and 14-day charts as sparklines
  add-lang [--force] <lang> <file>  install a stages.json file as a new language
  check <file>              validate a stages.json file without installing it
  help                      show this help

options:
  --version                 print the version";

/// The usage text plus the built-in courses line, generated from the
/// embedded course list so it can never drift from what actually ships
/// (D16) — an eleventh language updates this line by existing.
fn usage_text() -> String {
    let names: Vec<&str> = content::builtin_languages()
        .iter()
        .map(|(name, _)| *name)
        .collect();
    format!(
        "{USAGE}\n\ncourses built into this binary: {} — 'termocab list-langs' \
         shows every language available on this machine, including your own.",
        names.join(", ")
    )
}

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
    /// stdout closed mid-run — `termocab list-words french | head` must
    /// not panic. The reader of the pipe had enough, so the run ends
    /// the way cat and git do: nothing printed, exit code 141
    /// (128 + SIGPIPE). Re-raising the actual signal would need unsafe
    /// libc calls, which §6 rules out; 141 is what the shell reports
    /// either way (decision D6).
    BrokenPipe,
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Runtime(_) => 1,
            CliError::BrokenPipe => 141,
        }
    }

    /// Whether `main` should print nothing at all — a broken pipe is
    /// silent by definition (D6).
    pub fn quiet(&self) -> bool {
        matches!(self, CliError::BrokenPipe)
    }

    /// The message `main` prints after the "termocab: " prefix. Usage
    /// errors also point at `termocab help`, like git does.
    pub fn message(&self) -> String {
        match self {
            CliError::Usage(msg) => format!("{msg}\nrun 'termocab help' for usage."),
            CliError::Runtime(msg) => msg.clone(),
            CliError::BrokenPipe => String::new(),
        }
    }
}

/// One line to stdout, error-checked. Every line termocab prints goes
/// through here — `println!` panics when the pipe is gone
/// (`termocab list-words french | head`), which is the one way this
/// tool ever crashed on a healthy system (D6).
fn say(text: &str) -> Result<(), CliError> {
    let mut out = io::stdout().lock();
    match writeln!(out, "{text}") {
        Ok(()) => Ok(()),
        Err(e) => Err(stdout_error(e)),
    }
}

/// What a failed stdout write means: a closed pipe is the reader
/// saying "enough" (quiet exit 141, D6); anything else (disk full,
/// I/O error) is an ordinary runtime failure.
fn stdout_error(e: io::Error) -> CliError {
    match e.kind() {
        io::ErrorKind::BrokenPipe => CliError::BrokenPipe,
        _ => runtime(format!("could not write to stdout: {e}")),
    }
}

/// Extracts `--no-shuffle` / `--seed <n>` from a `start` argument list
/// (any position, like add-lang's `--force`) and returns the exercise
/// order plus the remaining positional arguments. Unknown `--` options
/// are usage errors rather than being eaten as language names. Without
/// either flag the exercises shuffle under a fresh OS seed — the
/// default the review asked for (D7).
fn parse_order_flags(rest: &[String]) -> Result<(ExerciseOrder, Vec<String>), CliError> {
    const USAGE_LINE: &str = "usage: termocab start [--no-shuffle|--seed <n>] <lang> [stage]";
    let mut positional = Vec::new();
    let mut order: Option<ExerciseOrder> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--no-shuffle" => {
                if order.is_some() {
                    return Err(usage(format!(
                        "--no-shuffle and --seed say opposite things\n{USAGE_LINE}"
                    )));
                }
                order = Some(ExerciseOrder::InOrder);
            }
            "--seed" => {
                let Some(value) = rest.get(i + 1) else {
                    return Err(usage(format!(
                        "'--seed' needs a number: --seed <n>\n{USAGE_LINE}"
                    )));
                };
                let seed = value.parse::<u64>().map_err(|_| {
                    usage(format!(
                        "--seed wants a number, not {value:?}\n{USAGE_LINE}"
                    ))
                })?;
                if order.is_some() {
                    return Err(usage(format!(
                        "--no-shuffle and --seed say opposite things\n{USAGE_LINE}"
                    )));
                }
                order = Some(ExerciseOrder::Shuffled(seed));
                i += 1; // the seed value is consumed, not positional
            }
            other if other.starts_with("--") => {
                return Err(usage(format!("unknown option: {other}\n{USAGE_LINE}")));
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    Ok((
        order.unwrap_or_else(|| ExerciseOrder::Shuffled(os_seed())),
        positional,
    ))
}

/// A fresh shuffle seed from the operating system: nanoseconds from
/// the system clock mixed with the process id. Not cryptographic —
/// just different on every run, which is all a vocabulary shuffle
/// needs. Lives here, at the CLI edge and unwitnessed by tests, for
/// the same reason the wall clock does; the session layer stays pure
/// and takes the seed as a value (D7).
fn os_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (u64::from(std::process::id()) << 32)
}

/// Whether session feedback gets ANSI color: stdout must be a
/// terminal (piping into a file or `grep` must stay plain — plain
/// text is the product), and $NO_COLOR must be unset or empty
/// (https://no-color.org, which termocab honors exactly).
fn colors_enabled() -> Color {
    if color_for(
        io::stdout().is_terminal(),
        std::env::var("NO_COLOR").ok().as_deref(),
    ) {
        Color::Ansi
    } else {
        Color::Plain
    }
}

/// The decision behind [`colors_enabled`], split out so the rule is
/// testable without a terminal: a tty wins unless NO_COLOR is set to
/// a non-empty value; an empty NO_COLOR does not count as set, per
/// the spec.
fn color_for(is_terminal: bool, no_color: Option<&str>) -> bool {
    is_terminal && !no_color.is_some_and(|v| !v.is_empty())
}

/// Candidate content directories, in lookup order — the first one that
/// actually holds the language wins (item 5 / decision D2):
///  1. $TERMOCAB_CONTENT_DIR — an explicit override (a directory that
///     directly contains <lang>/stages.json);
///  2. $XDG_DATA_HOME/termocab/languages, or ~/.local/share/termocab/languages
///     — where add-lang installs, so an installed binary finds its
///     content no matter the current directory;
///  3. ./languages — development convenience, so `cargo run` works.
fn content_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(dir) = std::env::var("TERMOCAB_CONTENT_DIR") {
        if !dir.is_empty() {
            dirs.push(PathBuf::from(dir));
        }
    }
    match std::env::var("XDG_DATA_HOME") {
        Ok(xdg) if !xdg.is_empty() => {
            dirs.push(Path::new(&xdg).join("termocab").join("languages"));
        }
        _ => {
            if let Ok(home) = std::env::var("HOME") {
                if !home.is_empty() {
                    dirs.push(Path::new(&home).join(".local/share/termocab/languages"));
                }
            }
        }
    }
    dirs.push(PathBuf::from("languages"));
    dirs
}

/// Where add-lang installs: always the XDG data location, never the
/// dev fallback or whatever directory the user happens to be in (the
/// old cwd-relative behavior — item 5).
fn install_dir() -> Result<PathBuf, CliError> {
    match std::env::var("XDG_DATA_HOME") {
        Ok(xdg) if !xdg.is_empty() => Ok(Path::new(&xdg).join("termocab").join("languages")),
        _ => match std::env::var("HOME") {
            Ok(home) if !home.is_empty() => {
                Ok(Path::new(&home).join(".local/share/termocab/languages"))
            }
            _ => Err(runtime(
                "could not determine where to install content: XDG_DATA_HOME and HOME are both unset",
            )),
        },
    }
}

/// The first candidate directory that holds `lang`'s stages.json.
fn stages_path(lang: &LangId) -> Option<PathBuf> {
    content_dirs()
        .into_iter()
        .map(|dir| dir.join(lang.as_str()).join("stages.json"))
        .find(|path| path.is_file())
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

/// Where "today" comes from. Real runs read the local calendar date
/// (see the [`time`] module); tests inject a fixed date so no test ever
/// depends on the wall clock (review item 6).
trait Clock {
    fn today(&self) -> String;
}

/// The real clock: the local date, DST included — see `cli/time.rs`.
struct LocalClock;

impl Clock for LocalClock {
    fn today(&self) -> String {
        time::today_local()
    }
}

/// A clock frozen at a fixed date, for tests.
#[cfg(test)]
struct FixedClock(&'static str);

#[cfg(test)]
impl Clock for FixedClock {
    fn today(&self) -> String {
        self.0.to_string()
    }
}

/// What main() hands us: the raw argument list after the program name.
pub fn run(args: &[String]) -> Result<(), CliError> {
    run_with(args, &LocalClock)
}

/// [`run`] with the clock injected — the seam tests use.
fn run_with(args: &[String], clock: &dyn Clock) -> Result<(), CliError> {
    let Some((cmd, rest)) = args.split_first() else {
        say(&usage_text())?;
        return Ok(());
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => {
            expect_no_args(rest, "help")?;
            say(&usage_text())?;
            Ok(())
        }
        "--version" | "-V" => {
            expect_no_args(rest, "--version")?;
            say(&format!("termocab {}", env!("CARGO_PKG_VERSION")))?;
            Ok(())
        }
        "start" => start(rest, clock),
        "review" => review(rest, clock),
        "list-words" => list_words(rest),
        "list-langs" => list_langs(rest),
        "progress" => progress(rest, clock),
        "streak" => streak(rest, clock),
        "stats" => stats(rest, clock),
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

/// start [--no-shuffle | --seed <n>] <lang> [stage] — run the next
/// uncompleted stage, or a chosen one. Records progress on pass. There
/// is no placement system yet, so every learner starts at Stage 1
/// (documented in the handoff, §9). Stages unlock strictly in order: an
/// explicit stage number may only be a completed stage (replay) or the
/// next uncompleted one (review item 7 / decision D4 — the original
/// spec's unlock condition, now enforced).
///
/// Exercises shuffle on every run by default — positions are not
/// vocabulary (D7). `--no-shuffle` keeps the authored order;
/// `--seed <n>` pins the order for reproducible runs and tests.
fn start(rest: &[String], clock: &dyn Clock) -> Result<(), CliError> {
    let (order, rest) = parse_order_flags(rest)?;
    if rest.is_empty() || rest.len() > 2 {
        return Err(usage(
            "'start' takes a language and an optional stage number, plus --no-shuffle or --seed <n>\nusage: termocab start [--no-shuffle|--seed <n>] <lang> [stage]",
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
                say(&format!(
                    "every stage in {lang} is complete. nothing left to start."
                ))?;
                return Ok(());
            }
        },
    };

    let mut term = StdioTerminal;
    let today = clock.today();
    // Snapshot what the run could change, so the file is written only
    // when something actually does: a failed or aborted run never saves,
    // and a passed replay on an already-recorded day rewrites nothing
    // (item 11 — the old code rewrote the file even on </dev/null runs).
    // A run that dies on a closed stdout is aborted the same way (D6).
    let stage_was_done = prog.is_stage_completed(stage.id);
    let day_was_recorded = prog.is_active_day(&today);
    let opts = RunOptions {
        order,
        color: colors_enabled(),
    };
    let outcome = session::run_and_record(&mut term, stage, &mut prog, &today, &opts)
        .map_err(stdout_error)?;
    let changed = outcome.passed() && !(stage_was_done && day_was_recorded);
    if changed {
        storage::save(&root, &lang, &prog).map_err(|e| runtime(e.to_string()))?;
        say("progress saved.")?;
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
fn review(rest: &[String], clock: &dyn Clock) -> Result<(), CliError> {
    let lang = expect_lang(rest, "review")?;
    let stages = load_lang(&lang)?;
    let root = config_root()?;
    let mut prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    let today = clock.today();
    let (mut srs, due) = due_reviews(&root, &lang, &stages, &prog, &today)?;

    if due == 0 {
        match srs.next_due(&today) {
            Some((date, n)) => {
                say(&format!(
                    "no cards due today. next review: {date} ({n} word(s))."
                ))?;
            }
            None => say(&format!(
                "no cards due for {lang} yet — complete a stage to start scheduling reviews."
            ))?,
        }
        return Ok(());
    }

    let vocabulary = reviewable_words(&stages, &prog);
    let mut term = StdioTerminal;
    let outcome = session::run_review(&mut term, &mut srs, &vocabulary, &today, colors_enabled())
        .map_err(stdout_error)?;

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
        say("review saved.")?;
    }
    Ok(())
}

/// list-words <lang> — every vocabulary entry across all stages.
fn list_words(rest: &[String]) -> Result<(), CliError> {
    let lang = expect_lang(rest, "list-words")?;
    let stages = load_lang(&lang)?;
    for stage in &stages {
        say(&format!(
            "Stage {} — {} [{}]",
            stage.id, stage.title, stage.level_hint
        ))?;
        for v in &stage.vocabulary {
            // alternatives surface in the listing so the learner knows
            // every accepted answer before a review grades them (D8)
            let also = if v.accepted_translations.is_empty() {
                String::new()
            } else {
                format!(" (also: {})", v.accepted_translations.join(", "))
            };
            say(&format!(
                "  {} ({}) — {}{}",
                v.word, v.pos, v.translation, also
            ))?;
            say(&format!("    e.g. {}", v.example))?;
        }
    }
    Ok(())
}

/// Where a listed language's content comes from — the last column of
/// `list-langs`, and the honest answer to "why does my edit do
/// nothing": the winning source is shown, not every candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LanguageSource {
    /// A stages.json in one of the candidate directories (D2) — the
    /// override dir, the XDG data location, or a checkout's ./languages.
    Dir(PathBuf),
    /// Compiled into the binary (D16).
    Builtin,
}

/// One row of `list-langs`: a language name, the source it resolves
/// from, and — when the content actually loads — its size.
#[derive(Debug, Clone)]
struct LanguageListing {
    name: String,
    source: LanguageSource,
    /// `(stages, words)` once loaded; `None` when the content is broken
    /// or the directory name is not even a valid language id — the row
    /// still lists, with the reason instead of the counts.
    size: Option<(usize, usize)>,
    /// The reason `size` is `None`, shown instead of counts.
    problem: Option<String>,
}

/// Every language this run can see, alphabetically, one entry per name:
/// the candidate directories (D2) in lookup order — first hit wins, the
/// same rule `start` follows — then the built-in courses (D16) for any
/// name not already found on disk. Unreadable or missing directories
/// contribute nothing; a directory whose name could never be a language
/// id still lists, flagged, because that is exactly the install nobody
/// can find.
fn available_languages() -> Vec<LanguageListing> {
    let mut found: std::collections::BTreeMap<String, LanguageSource> =
        std::collections::BTreeMap::new();
    for dir in content_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue; // not there or not readable — nothing to list
        };
        for entry in entries.flatten() {
            let has_stages = entry.path().join("stages.json").is_file();
            if !has_stages {
                continue;
            }
            let Some(name) = entry.file_name().into_string().ok() else {
                continue; // not UTF-8 — cannot be typed as a language anyway
            };
            found
                .entry(name)
                .or_insert_with(|| LanguageSource::Dir(entry.path()));
        }
    }
    for (name, _) in content::builtin_languages() {
        found
            .entry((*name).to_string())
            .or_insert(LanguageSource::Builtin);
    }
    found
        .into_iter()
        .map(|(name, source)| {
            // Load each listing so `list-langs` reports real sizes —
            // and flags broken content instead of hiding it.
            let listing = LanguageListing {
                size: None,
                problem: None,
                name: name.clone(),
                source,
            };
            let Ok(lang) = LangId::parse(&name) else {
                return LanguageListing {
                    problem: Some("unusable name (needs a-z, 0-9, '-' or '_')".into()),
                    ..listing
                };
            };
            let loaded = match &listing.source {
                LanguageSource::Dir(dir) => {
                    content::load_language(dir.join("stages.json"), Some(&lang))
                }
                LanguageSource::Builtin => {
                    content::load_builtin(&lang).expect("a built-in source always has the course")
                }
            };
            match loaded {
                Ok(content) => {
                    let words = content.stages.iter().map(|s| s.vocabulary.len()).sum();
                    LanguageListing {
                        size: Some((content.stages.len(), words)),
                        ..listing
                    }
                }
                // The violations are multi-line; the row stays one line
                // and the trailing note says how to see them.
                Err(_) => LanguageListing {
                    problem: Some("invalid content".into()),
                    ..listing
                },
            }
        })
        .collect()
}

/// list-langs — the discovery verb: every available language, its size,
/// and where it resolves from. The answer to the fresh-install question
/// "what can I even start?", in plain text.
fn list_langs(rest: &[String]) -> Result<(), CliError> {
    expect_no_args(rest, "list-langs")?;
    let listings = available_languages();
    if listings.is_empty() {
        // Unreachable while any course ships built-in (D16) — kept for
        // the day the list is empty on purpose.
        say("no languages available — add one with: termocab add-lang <lang> <stages.json>")?;
        return Ok(());
    }
    let name_width = listings.iter().map(|l| l.name.len()).max().unwrap_or(0);
    let detail_width = listings
        .iter()
        .map(|l| size_text(l).len())
        .max()
        .unwrap_or(0);
    for listing in &listings {
        say(&format!(
            "{:<name_width$}  {:<detail_width$}  {}",
            listing.name,
            size_text(listing),
            source_text(&listing.source),
        ))?;
    }
    if listings.iter().any(|l| l.problem.is_some()) {
        say("a language with a problem cannot be started — \
             'termocab check <file>' on its stages.json explains why.")?;
    }
    Ok(())
}

/// The size column: counts when the content loads, the problem when it
/// does not.
fn size_text(listing: &LanguageListing) -> String {
    if let Some(problem) = &listing.problem {
        return problem.clone();
    }
    let (stages, words) = listing
        .size
        .expect("a listing without a problem has a size");
    format!("{stages} stage(s), {words} word(s)")
}

/// The source column: the built-ins say so; a directory shows its path,
/// exactly where the content is being read from.
fn source_text(source: &LanguageSource) -> String {
    match source {
        LanguageSource::Builtin => "built-in".to_string(),
        LanguageSource::Dir(dir) => dir.display().to_string(),
    }
}

/// progress <lang> — completed stages, current streak, due reviews.
fn progress(rest: &[String], clock: &dyn Clock) -> Result<(), CliError> {
    let lang = expect_lang(rest, "progress")?;
    let stages = load_lang(&lang)?;
    let root = config_root()?;
    let prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    let today = clock.today();
    let done = prog.completed_stages();
    say(&format!(
        "{lang}: {}/{} stage(s) complete",
        done.len(),
        stages.len()
    ))?;
    for stage in &stages {
        let mark = if prog.is_stage_completed(stage.id) {
            "[x]"
        } else {
            "[ ]"
        };
        say(&format!(
            "  {mark} Stage {} — {} [{}]",
            stage.id, stage.title, stage.level_hint
        ))?;
    }
    match prog.current_stage_id(&stages) {
        Some(id) => say(&format!("  next up: stage {id}"))?,
        None => say("  all stages complete.")?,
    }
    say(&format!("  streak: {} day(s)", prog.streak_on(&today)))?;
    let (_, due) = due_reviews(&root, &lang, &stages, &prog, &today)?;
    say(&format!(
        "  reviews due today: {due} word(s) — run 'termocab review {lang}'"
    ))?;
    Ok(())
}

/// streak <lang> — print just the streak count, script-friendly.
fn streak(rest: &[String], clock: &dyn Clock) -> Result<(), CliError> {
    let lang = expect_lang(rest, "streak")?;
    let root = config_root()?;
    let prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    say(&format!("{}", prog.streak_on(&clock.today())))?;
    Ok(())
}

/// stats <lang> — the learner's numbers at a glance: progress, words,
/// the Leitner box spread, the coming review load and the last two
/// weeks of activity, each chart as a one-line Unicode sparkline.
/// Plain text on purpose: `termocab stats french` stays cat-able and
/// grep-able, no color.
fn stats(rest: &[String], clock: &dyn Clock) -> Result<(), CliError> {
    let lang = expect_lang(rest, "stats")?;
    let stages = load_lang(&lang)?;
    let root = config_root()?;
    let prog = storage::load(&root, &lang).map_err(|e| runtime(e.to_string()))?;
    let today = clock.today();
    let (srs, due_today) = due_reviews(&root, &lang, &stages, &prog, &today)?;

    let done = prog.completed_stages();
    say(&format!(
        "{lang} — {}/{} stage(s) complete, streak {} day(s)",
        done.len(),
        stages.len(),
        prog.streak_on(&today)
    ))?;
    let in_content: usize = stages.iter().map(|s| s.vocabulary.len()).sum();
    let scheduled = srs.cards().len();
    say(&format!(
        "words: {in_content} in content, {scheduled} scheduled, {due_today} due today"
    ))?;

    if scheduled == 0 {
        say("no cards scheduled yet — complete a stage to start scheduling reviews.")?;
        return Ok(());
    }

    // box spread: how well the scheduled words are known, 1 (fresh)
    // to 6 (settled). Counts after the chart, chart first.
    let mut boxes = [0usize; srs::MAX_BOX as usize];
    for (_, card) in srs.cards() {
        boxes[(card.box_number - 1) as usize] += 1;
    }
    let counts: Vec<String> = boxes.iter().map(|n| n.to_string()).collect();
    say(&format!(
        "boxes 1-{}: {}  ({})",
        srs::MAX_BOX,
        sparkline(&boxes),
        counts.join(" ")
    ))?;

    // review load: today (overdue included), then the next 13 days —
    // when to come back
    let mut due_next: Vec<usize> = vec![due_today];
    for i in 1..14 {
        let due = srs::add_days(&today, i)
            .map(|d| srs.cards().iter().filter(|(_, c)| c.due == d).count())
            .unwrap_or(0);
        due_next.push(due);
    }
    say(&format!(
        "reviews due, next 14 days (today first): {}",
        sparkline(&due_next)
    ))?;

    // activity: the last 14 days, oldest first — the streak made
    // visible. Only shown once there is anything to show.
    if !prog.active_days().is_empty() {
        let mut active: Vec<usize> = Vec::with_capacity(14);
        for i in (0..14).rev() {
            let active_day = srs::add_days(&today, -i)
                .map(|d| prog.is_active_day(&d))
                .unwrap_or(false);
            active.push(usize::from(active_day));
        }
        say(&format!(
            "active days, last 14 (oldest first): {}",
            sparkline(&active)
        ))?;
    }
    Ok(())
}

/// A one-line bar chart: each value becomes one of the eight Unicode
/// eighth blocks, ▁ (zero) to █ (the maximum). Integer math only —
/// `(v * 7) / max`, so the maximum is always a full block and an
/// all-zero series is the flattest line, not a division by zero.
fn sparkline(values: &[usize]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = values.iter().copied().max().unwrap_or(0);
    values
        .iter()
        .map(|v| {
            // 0 when the series is all zeros (checked_div: no /0); the
            // maximum value is always the full block
            let level = (v * 7).checked_div(max).map_or(0, |q| q.min(7));
            BLOCKS[level]
        })
        .collect()
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

    let dest = install_dir()?.join(lang.as_str()).join("stages.json");
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
    // Installing over a shipped course is legal and intended (D16: the
    // installed copy takes precedence) — but it deserves a note, so
    // nobody wonders later why their french no longer matches the docs.
    let over_builtin = if content::builtin_text(&lang).is_some() {
        format!(
            " — this installed copy now takes precedence over the \
             built-in '{lang}' course"
        )
    } else {
        String::new()
    };
    say(&format!(
        "{verb} language '{lang}' — {} stage(s) installed at {}{over_builtin}",
        loaded.stages.len(),
        dest.display()
    ))?;
    Ok(())
}

fn load_lang(lang: &LangId) -> Result<Vec<crate::engine::Stage>, CliError> {
    // The lookup chain, first hit wins: candidate dirs (D2), then the
    // courses compiled into the binary (D16). Built-ins are the last
    // resort so an installed binary works from any directory with zero
    // setup — and so an installed file, an override dir or a dev
    // checkout still takes precedence over what ships.
    if let Some(path) = stages_path(lang) {
        return match content::load_language(&path, Some(lang)) {
            Ok(loaded) => Ok(loaded.stages),
            Err(e) => Err(runtime(format!("{lang}: {e}"))),
        };
    }
    match content::load_builtin(lang) {
        Some(Ok(loaded)) => Ok(loaded.stages),
        Some(Err(e)) => Err(runtime(format!("{lang}: {e}"))),
        None => Err(missing_language_error(lang)),
    }
}

/// The no-content error, written for the learner who just wants to
/// start something: where termocab looked, everything that IS
/// available, a suggestion for the near-miss, and how to add your own.
fn missing_language_error(lang: &LangId) -> CliError {
    let searched = content_dirs()
        .into_iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let available: Vec<String> = available_languages().into_iter().map(|l| l.name).collect();
    let mut lines = vec![format!(
        "no content for language '{lang}' — searched: {searched}"
    )];
    if !available.is_empty() {
        lines.push(format!(
            "available languages: {} ('termocab list-langs' shows where each comes from)",
            available.join(", ")
        ));
    }
    if let Some(suggestion) = did_you_mean(lang.as_str(), &available) {
        lines.push(format!(
            "did you mean '{suggestion}'? start it with: termocab start {suggestion}"
        ));
    }
    lines.push(format!(
        "add your own with: termocab add-lang {lang} <stages.json>"
    ));
    runtime(lines.join("\n"))
}

/// The closest available language to a name that matched nothing — the
/// typo rescue. A name within two edits of one candidate, or a prefix
/// of exactly one candidate, suggests that candidate; ties go to the
/// alphabetically first. `None` when nothing is close enough to guess.
fn did_you_mean(typed: &str, available: &[String]) -> Option<String> {
    let mut best: Option<(usize, &String)> = None;
    for candidate in available {
        let distance = edit_distance(typed, candidate.as_str());
        if distance > 2 {
            continue;
        }
        if best.map_or(true, |(d, _)| distance < d) {
            best = Some((distance, candidate));
        }
    }
    if let Some((_, candidate)) = best {
        return Some(candidate.clone());
    }
    // no near match — a prefix of exactly one candidate still points
    // somewhere useful: "fre" -> "french"
    let prefixed: Vec<&String> = available.iter().filter(|c| c.starts_with(typed)).collect();
    match prefixed.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

/// Levenshtein distance over characters, the classic two-row form.
/// Names are at most 32 characters ([`crate::lang::LangId`]), so the
/// quadratic cost is trivial and no dependency is needed (§6).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = (current[j] + 1).min(previous[j + 1] + 1).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
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
            say(&format!(
                "ok: {} stage(s), {exercises} exercise(s), language '{}'",
                loaded.stages.len(),
                loaded.language
            ))?;
            Ok(())
        }
        Err(e) => Err(runtime(format!("{file}: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            (vec!["list-langs", "french"], "termocab list-langs"),
            (
                vec!["add-lang", "french"],
                "termocab add-lang [--force] <lang> <file>",
            ),
            (vec!["check", "a.json", "b.json"], "termocab check <file>"),
            (
                vec!["start"],
                "termocab start [--no-shuffle|--seed <n>] <lang> [stage]",
            ),
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
    fn fixed_clock_pins_today_for_tests() {
        // The seam (review item 6): handlers take the clock, tests
        // freeze the date. The behavioral date logic itself is covered
        // by the session/progress tests, which inject date strings.
        assert_eq!(FixedClock("2026-10-30").today(), "2026-10-30");
    }

    #[test]
    fn broken_pipe_maps_to_a_quiet_141_exit() {
        // The D6 mapping: EPIPE is silent success-as-far-as-the-reader-
        // is-concerned; every other write failure is an ordinary error.
        let e = stdout_error(io::Error::from(io::ErrorKind::BrokenPipe));
        assert_eq!(e, CliError::BrokenPipe);
        assert_eq!(e.exit_code(), 141);
        assert!(e.quiet());
        assert!(e.message().is_empty());

        let e = stdout_error(io::Error::other("no space left on device"));
        assert_eq!(e.exit_code(), 1);
        assert!(!e.quiet());
        assert!(e.message().contains("could not write to stdout"));
    }

    #[test]
    fn order_flags_parse_in_any_position() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // --no-shuffle alone, before or after the positionals
        let (order, rest) = parse_order_flags(&args(&["--no-shuffle", "french"])).unwrap();
        assert_eq!(order, ExerciseOrder::InOrder);
        assert_eq!(rest, vec!["french".to_string()]);
        let (order, rest) = parse_order_flags(&args(&["french", "2", "--no-shuffle"])).unwrap();
        assert_eq!(order, ExerciseOrder::InOrder);
        assert_eq!(rest, vec!["french".to_string(), "2".to_string()]);

        // --seed consumes its value, wherever both sit
        let (order, rest) = parse_order_flags(&args(&["--seed", "42", "french", "1"])).unwrap();
        assert_eq!(order, ExerciseOrder::Shuffled(42));
        assert_eq!(rest, vec!["french".to_string(), "1".to_string()]);
        let (order, rest) = parse_order_flags(&args(&["french", "--seed", "7", "1"])).unwrap();
        assert_eq!(order, ExerciseOrder::Shuffled(7));
        assert_eq!(rest, vec!["french".to_string(), "1".to_string()]);
    }

    #[test]
    fn order_flags_reject_the_wrong_and_the_unknown() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        for bad in [
            vec!["--no-shuffle", "--seed", "1", "french"], // opposite asks
            vec!["french", "--seed"],                      // value missing
            vec!["french", "--seed", "abc"],               // not a number
            vec!["french", "--shuffel"],                   // typo, not eaten silently
        ] {
            let err = parse_order_flags(&args(&bad)).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{bad:?}");
            assert!(
                err.message().contains("termocab start"),
                "{bad:?}: {}",
                err.message()
            );
        }
    }

    #[test]
    fn color_needs_a_terminal_and_a_missing_no_color() {
        // no-color.org, to the letter: set AND non-empty disables;
        // empty counts as unset.
        assert!(color_for(true, None));
        assert!(color_for(true, Some("")));
        assert!(!color_for(true, Some("1")));
        assert!(!color_for(true, Some("always")));
        // a pipe or a file never gets color, whatever the env says
        assert!(!color_for(false, None));
        assert!(!color_for(false, Some("")));
    }

    #[test]
    fn sparkline_maps_values_onto_the_eighth_blocks() {
        // zero is the lowest block, the maximum is a full block, and
        // the steps in between are integer math: v * 7 / max
        assert_eq!(sparkline(&[0, 1, 2, 3, 4]), "▁▂▄▆█");
        // every value at the maximum
        assert_eq!(sparkline(&[5, 5, 5]), "███");
        // all zeros does not divide by zero
        assert_eq!(sparkline(&[0, 0, 0, 0]), "▁▁▁▁");
        // empty series, empty chart
        assert_eq!(sparkline(&[]), "");
        // one lonely card is a full block
        assert_eq!(sparkline(&[1]), "█");
    }

    #[test]
    fn read_only_commands_run_against_the_injected_clock() {
        // Dispatch threads the clock through to the handlers; a
        // read-only verb completes without touching the wall clock.
        let _env = crate::testenv::env_lock();
        let data = crate::testenv::TempDir::new("clock-data");
        std::env::set_var("TERMOCAB_CONTENT_DIR", data.path());
        std::fs::create_dir_all(data.path().join("french")).unwrap();
        std::fs::write(data.path().join("french/stages.json"), VALID_STAGES).unwrap();
        let cfg = crate::testenv::TempDir::new("clock-cfg");
        std::env::set_var("XDG_CONFIG_HOME", cfg.path());
        std::fs::create_dir_all(cfg.path().join("termocab/progress")).unwrap();
        std::fs::write(
            cfg.path().join("termocab/progress/french.txt"),
            "stage 1\nday 2024-01-01\n",
        )
        .unwrap();
        run_with(&args(&["progress", "french"]), &FixedClock("2024-01-01")).unwrap();
        run_with(&args(&["streak", "french"]), &FixedClock("2024-01-01")).unwrap();
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

    #[test]
    fn content_lookup_order_is_override_then_xdg_then_home() {
        let _env = crate::testenv::env_lock();
        let override_dir = crate::testenv::TempDir::new("lookup-override");
        let xdg_dir = crate::testenv::TempDir::new("lookup-xdg");
        std::env::set_var("TERMOCAB_CONTENT_DIR", override_dir.path());
        std::env::set_var("XDG_DATA_HOME", xdg_dir.path());

        let klingon = LangId::parse("klingon").unwrap();
        // nowhere (klingon exists in no candidate, including the
        // ./languages dev fallback of this repo checkout)
        assert!(stages_path(&klingon).is_none());

        // in the XDG location only -> found there
        let xdg_lang = xdg_dir.path().join("termocab/languages/klingon");
        std::fs::create_dir_all(&xdg_lang).unwrap();
        std::fs::write(xdg_lang.join("stages.json"), "{}").unwrap();
        assert_eq!(
            stages_path(&klingon),
            Some(
                xdg_dir
                    .path()
                    .join("termocab/languages/klingon/stages.json")
            )
        );

        // in the override too -> the override wins
        let over_lang = override_dir.path().join("klingon");
        std::fs::create_dir_all(&over_lang).unwrap();
        std::fs::write(over_lang.join("stages.json"), "{}").unwrap();
        assert_eq!(
            stages_path(&klingon),
            Some(override_dir.path().join("klingon/stages.json"))
        );

        // without XDG_DATA_HOME the candidate comes from $HOME
        std::env::remove_var("XDG_DATA_HOME");
        let home = crate::testenv::TempDir::new("lookup-home");
        std::env::set_var("HOME", home.path());
        assert!(content_dirs().contains(&home.path().join(".local/share/termocab/languages")));
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
        // add-lang installs into the XDG data location, hermetically
        // pointed at a temp dir for this test (item 5).
        let data = crate::testenv::TempDir::new("addlang-data");
        std::env::set_var("XDG_DATA_HOME", data.path());
        let installed = data.path().join("termocab/languages/french/stages.json");

        let src = crate::testenv::TempDir::new("addlang-src");
        let valid = src.path().join("stages.json");
        std::fs::write(&valid, VALID_STAGES).unwrap();
        let valid = valid.to_str().unwrap().to_string();

        // happy path: installs under <data>/termocab/languages/<lang>/
        run(&args(&["add-lang", "french", &valid])).unwrap();
        assert!(installed.exists());

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
        assert!(!data.path().join("termocab/languages/german").exists());

        // and the installed language actually loads through `progress`,
        // found via the XDG data location (the config dir holds progress)
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
                    accepted_translations: vec![],
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

    #[test]
    fn usage_text_lists_the_builtin_courses() {
        // The trailing line is generated from the embedded list, so it
        // names exactly what ships — an eleventh language updates it by
        // existing (D16).
        let text = usage_text();
        assert!(text.contains("usage: termocab <command>"), "{text}");
        assert!(text.contains("courses built into this binary:"), "{text}");
        for (name, _) in content::builtin_languages() {
            assert!(text.contains(name), "{name} missing from the usage text");
        }
    }

    #[test]
    fn version_takes_no_arguments() {
        // The output itself is pinned by the binary-level test (a unit
        // test cannot capture its own stdout); here only the arity.
        let err = run(&args(&["--version", "extra"])).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.message().contains("takes no arguments"));
        let err = run(&args(&["-V", "extra"])).unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn did_you_mean_suggests_near_misses_and_unique_prefixes() {
        let available =
            |langs: &[&str]| -> Vec<String> { langs.iter().map(|s| s.to_string()).collect() };
        let langs = available(&["french", "german", "italian"]);
        // one substitution away
        assert_eq!(
            did_you_mean("frensh", &langs).as_deref(),
            Some("french"),
            "the reported typo"
        );
        // two edits away still suggests
        assert_eq!(did_you_mean("gemran", &langs).as_deref(), Some("german"));
        // a prefix of exactly one candidate points at it
        assert_eq!(did_you_mean("fre", &langs).as_deref(), Some("french"));
        // nothing close, nothing prefixed
        assert_eq!(did_you_mean("klingon", &langs), None);
        // an ambiguous prefix is not a guess
        let langs = available(&["spanish", "swedish"]);
        assert_eq!(did_you_mean("s", &langs), None);
        assert_eq!(did_you_mean("span", &langs).as_deref(), Some("spanish"));
    }

    #[test]
    fn edit_distance_counts_the_classic_cases() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("french", "french"), 0);
        assert_eq!(edit_distance("frensh", "french"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("german", "germany"), 1);
    }

    #[test]
    fn missing_language_error_names_what_is_available() {
        // Under the env lock, from the crate root: ./languages holds the
        // ten shipped courses, so the error must offer them — the exact
        // fresh-install confusion D16 answers.
        let _env = crate::testenv::env_lock();
        let data = crate::testenv::TempDir::new("missing-data");
        std::env::set_var("TERMOCAB_CONTENT_DIR", data.path());
        let home = crate::testenv::TempDir::new("missing-home");
        std::env::set_var("HOME", home.path());

        let err = missing_language_error(&LangId::parse("klingon").unwrap());
        assert_eq!(err.exit_code(), 1);
        let msg = err.message();
        assert!(msg.contains("no content for language 'klingon'"), "{msg}");
        assert!(msg.contains("searched:"), "{msg}");
        assert!(msg.contains("available languages:"), "{msg}");
        assert!(msg.contains("french"), "{msg}");
        assert!(!msg.contains("did you mean"), "{msg}");

        // the typo case points at the fix
        let err = missing_language_error(&LangId::parse("frensh").unwrap());
        let msg = err.message();
        assert!(msg.contains("did you mean 'french'?"), "{msg}");
        assert!(
            msg.contains("termocab start french"),
            "the suggestion is runnable as-is: {msg}"
        );
    }

    #[test]
    fn listing_rows_show_sizes_problems_and_sources() {
        let sized = LanguageListing {
            name: "test".into(),
            source: LanguageSource::Builtin,
            size: Some((2, 5)),
            problem: None,
        };
        assert_eq!(size_text(&sized), "2 stage(s), 5 word(s)");
        assert_eq!(source_text(&sized.source), "built-in");

        let broken = LanguageListing {
            size: None,
            problem: Some("invalid content".into()),
            ..sized.clone()
        };
        assert_eq!(size_text(&broken), "invalid content");

        let installed = LanguageListing {
            source: LanguageSource::Dir(PathBuf::from("/x/termocab/languages/test")),
            ..broken
        };
        assert_eq!(source_text(&installed.source), "/x/termocab/languages/test");
    }

    #[test]
    fn available_languages_prefers_earlier_dirs_and_falls_back_to_builtins() {
        // The listing rule mirrors the loader's (D2 then D16): a
        // language found in an earlier candidate directory never shows
        // its later duplicates, and a built-in appears only when no
        // directory holds the language. Needs the env lock because the
        // candidate list comes from the environment.
        let _env = crate::testenv::env_lock();
        let override_dir = crate::testenv::TempDir::new("list-override");
        let data_dir = crate::testenv::TempDir::new("list-data");
        std::env::set_var("TERMOCAB_CONTENT_DIR", override_dir.path());
        std::env::set_var("XDG_DATA_HOME", data_dir.path());
        // neutralize ./languages — but cwd cannot change under the test
        // process safely, so assert on names that exist nowhere on disk
        // plus one that exists in the override dir only, and on french,
        // which the checkout's ./languages DOES hold: the rule to check
        // is that the override wins over every later candidate,
        // including ./languages, for a language they all carry.
        let custom = override_dir.path().join("zulu");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(custom.join("stages.json"), one_word_course("zulu")).unwrap();
        let shadow_french = override_dir.path().join("french");
        std::fs::create_dir_all(&shadow_french).unwrap();
        std::fs::write(shadow_french.join("stages.json"), one_word_course("french")).unwrap();

        let listings = available_languages();
        let by_name = |n: &str| listings.iter().find(|l| l.name == n);

        let zulu = by_name("zulu").expect("zulu listed");
        assert_eq!(
            zulu.source,
            LanguageSource::Dir(custom.clone()),
            "the only holder wins"
        );
        assert_eq!(zulu.size, Some((1, 1)), "zulu's size is real: {zulu:?}");

        // french: the override beats ./languages (same dir earlier in
        // the chain) and the built-in — one row, the override's
        let french = by_name("french").expect("french listed");
        assert_eq!(
            french.source,
            LanguageSource::Dir(shadow_french.clone()),
            "earlier directory wins over ./languages and the built-in"
        );

        // klingon ships nowhere: the built-ins fill only real names, and
        // klingon is not among them
        assert!(by_name("klingon").is_none());

        // and a built-in-only language is listed from the binary —
        // spanish is in ./languages here (crate root), so use the fact
        // that the listing includes every builtin name somewhere
        for (name, _) in content::builtin_languages() {
            assert!(by_name(name).is_some(), "{name} must be listed");
        }
    }

    /// A one-stage, one-word course for a given language name —
    /// for listing tests that need real, loadable content.
    fn one_word_course(lang: &str) -> String {
        format!(
            r#"{{
        "language": "{lang}",
        "stages": [
          {{ "id": 1, "title": "One", "level_hint": "beginner",
            "vocabulary": [
              {{ "word": "sawubona", "translation": "hello", "pos": "x", "example": "Sawubona!" }}
            ],
            "exercises": [
              {{ "type": "translation", "prompt": "sawubona", "answer": "hello" }}
            ] }}
        ]
      }}"#
        )
    }
}
