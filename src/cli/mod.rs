//! CLI — git-style direct subcommands, hand-rolled from std::env::args().
//!
//! No CLI-parsing crate: with six verbs and one optional flag, a match
//! over the argument list is clearer than a dependency (§6 of the
//! handoff). Verbs:
//!   termocab start <lang> [stage]   run the next (or a given) stage
//!   termocab list-words <lang>      all vocabulary in a language
//!   termocab progress <lang>        completed stages + streak
//!   termocab streak <lang>          just the streak number
//!   termocab add-lang <lang> <file> install a stages.json as a language
//!   termocab help                   usage text
//!
//! Exit codes follow the Unix habits git users expect: 0 success, 1
//! runtime failure, 2 usage error (same as git).

use crate::content;
use crate::session::{self, StdioTerminal};
use crate::storage;
use std::path::{Path, PathBuf};
use std::process::exit;

const USAGE: &str = "\
termocab — language learning for terminal enthusiasts

usage: termocab <command> [args]

commands:
  start <lang> [stage]     run the next uncompleted stage (or a specific one)
  list-words <lang>         list all vocabulary in a language's stages
  progress <lang>           show completed stages and current streak
  streak <lang>             print the current streak in days
  add-lang <lang> <file>    install a stages.json file as a new language
  help                      show this help";

/// Where language content lives. `$TERMCOCAB_CONTENT_DIR` overrides the
/// default, which is `languages/` relative to the current directory —
/// the same place `cargo run` finds it.
fn content_dir() -> PathBuf {
    match std::env::var("TERMCOCAB_CONTENT_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from("languages"),
    }
}

fn stages_path(lang: &str) -> PathBuf {
    content_dir().join(lang).join("stages.json")
}

/// "Today" as YYYY-MM-DD from the TZ-local clock, without a time crate:
/// the unix-seconds-to-civil-date math is a handful of lines (Howard
/// Hinnant's algorithm) and avoids pulling chrono in for one value.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + local_utc_offset_seconds();
    civil_from_unix(secs)
}

/// Best-effort UTC offset for the local timezone. std can't read the
/// TZ database, so we probe a few fixed offsets is out of scope; we use
/// the offset implied by the difference between localtime-formatted…
/// which std also can't do. Simplest honest answer: read $TZ offsets is
/// overkill — we use UTC unless the caller sets TZHOURS.
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
pub fn run(args: &[String]) {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{USAGE}");
        return;
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => println!("{USAGE}"),
        "start" => start(rest),
        "list-words" => list_words(rest),
        "progress" => progress(rest),
        "streak" => streak(rest),
        "add-lang" => add_lang(rest),
        other => usage_error(&format!("unknown command: {other}")),
    }
}

fn usage_error(msg: &str) -> ! {
    eprintln!("termocab: {msg}");
    eprintln!("run 'termocab help' for usage.");
    exit(2);
}

fn expect_lang(rest: &[String], cmd: &str) -> String {
    let Some(lang) = rest.first() else {
        usage_error(&format!("'{cmd}' needs a language, e.g. 'termocab {cmd} french'"));
    };
    if rest.len() > 2 {
        usage_error(&format!("'{cmd}' takes at most one extra argument"));
    }
    lang.clone()
}

/// start <lang> [stage] — run the next uncompleted stage, or a chosen
/// one. Records progress on pass. There is no placement system yet, so
/// every learner starts at Stage 1 (documented in the handoff, §9).
fn start(rest: &[String]) {
    if rest.is_empty() || rest.len() > 2 {
        usage_error("'start' needs a language and optionally a stage number");
    }
    let lang = rest[0].clone();
    let stages = load_lang(&lang);

    let mut prog = match storage::load(&lang) {
        Ok(p) => p,
        Err(e) => die(&e.to_string()),
    };

    let stage = match rest.get(1) {
        Some(n) => match n.parse::<u32>() {
            Ok(id) => stages
                .iter()
                .find(|s| s.id == id)
                .unwrap_or_else(|| die(&format!("no stage {id} in {lang}"))),
            Err(_) => usage_error("stage must be a number, e.g. 'termocab start french 1'"),
        },
        None => match prog.next_stage(&stages) {
            Some(s) => s,
            None => {
                println!("every stage in {lang} is complete. nothing left to start.");
                return;
            }
        },
    };

    let mut term = StdioTerminal;
    let today = today();
    let outcome = session::run_and_record(&mut term, stage, &mut prog, &today);
    if let Err(e) = storage::save(&lang, &prog) {
        die(&e.to_string());
    }
    if outcome.passed() {
        println!("progress saved.");
    }
}

/// list-words <lang> — every vocabulary entry across all stages.
fn list_words(rest: &[String]) {
    let lang = expect_lang(rest, "list-words");
    let stages = load_lang(&lang);
    for stage in &stages {
        println!("Stage {} — {} [{}]", stage.id, stage.title, stage.level_hint);
        for v in &stage.vocabulary {
            println!("  {} ({}) — {}", v.word, v.pos, v.translation);
            println!("    e.g. {}", v.example);
        }
    }
}

/// progress <lang> — completed stages, current streak.
fn progress(rest: &[String]) {
    let lang = expect_lang(rest, "progress");
    let stages = load_lang(&lang);
    let prog = match storage::load(&lang) {
        Ok(p) => p,
        Err(e) => die(&e.to_string()),
    };
    let done = prog.completed_stages();
    println!("{lang}: {}/{} stage(s) complete", done.len(), stages.len());
    for stage in &stages {
        let mark = if prog.is_stage_completed(stage.id) { "[x]" } else { "[ ]" };
        println!("  {mark} Stage {} — {} [{}]", stage.id, stage.title, stage.level_hint);
    }
    match prog.current_stage_id(&stages) {
        Some(id) => println!("  next up: stage {id}"),
        None => println!("  all stages complete."),
    }
    println!("  streak: {} day(s)", prog.streak_on(&today()));
}

/// streak <lang> — print just the streak count, script-friendly.
fn streak(rest: &[String]) {
    let lang = expect_lang(rest, "streak");
    let prog = match storage::load(&lang) {
        Ok(p) => p,
        Err(e) => die(&e.to_string()),
    };
    println!("{}", prog.streak_on(&today()));
}

/// add-lang <lang> <file> — validate and copy a stages.json into the
/// content dir, so the next `start` can find it.
fn add_lang(rest: &[String]) {
    if rest.len() != 2 {
        usage_error("'add-lang' needs a language and a stages.json file path");
    }
    let lang = &rest[0];
    let src = Path::new(&rest[1]);

    // Validate before installing: it must parse as our content format.
    let loaded = match content::load_language(src) {
        Ok(s) => s,
        Err(e) => die(&format!("not a valid stages file: {e}")),
    };
    if loaded.is_empty() {
        die("refusing to add a language with zero stages");
    }

    let dest = stages_path(lang);
    if let Some(dir) = dest.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            die(&format!("could not create content dir: {e}"));
        }
    }
    if let Err(e) = std::fs::copy(src, &dest) {
        die(&format!("could not install language file: {e}"));
    }
    println!(
        "added language '{lang}' — {} stage(s) installed at {}",
        loaded.len(),
        dest.display()
    );
}

fn load_lang(lang: &str) -> Vec<crate::engine::Stage> {
    let path = stages_path(lang);
    match content::load_language(&path) {
        Ok(s) => s,
        Err(content::LoadError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => die(&format!(
            "no content for language '{lang}'. add one with: termocab add-lang {lang} <stages.json>"
        )),
        Err(e) => die(&format!("{lang}: {e}")),
    }
}

fn die(msg: &str) -> ! {
    eprintln!("termocab: {msg}");
    exit(1);
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

    #[test]
    fn usage_needs_no_args() {
        // run with no args prints usage and exits 0 — smoke-checked by
        // main's test harness instead of here, since run() prints.
    }
}
