//! Binary-level integration tests (review item 15): drive the real
//! termocab binary as a subprocess with hermetic HOME/XDG/content
//! directories, so lookup rules, exit codes, stdin handling and file
//! effects are exercised exactly as a user would see them. No test
//! crate — std::process::Command plus temp dirs.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A scratch area that removes itself when dropped: `home/` acts as
/// $HOME, `cwd/` is the working directory the binary runs from (an
/// empty directory, so the ./languages dev fallback never fires).
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "termocab-it-{}-{tag}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("home")).expect("create home");
        std::fs::create_dir_all(root.join("cwd")).expect("create cwd");
        Self { root }
    }

    fn root(&self) -> PathBuf {
        self.root.clone()
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn cwd(&self) -> PathBuf {
        self.root.join("cwd")
    }

    /// Writes a stages.json into the scratch area and returns its path.
    fn stages_file(&self, name: &str, json: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, json).expect("write stages file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The binary under test, in a clean environment: only $HOME is set, the
/// working directory is an empty scratch dir. Each test adds the env it
/// needs (XDG_CONFIG_HOME, TERMOCAB_CONTENT_DIR, …).
fn termocab(scratch: &Scratch) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_termocab"));
    cmd.env_clear()
        .env("HOME", scratch.home())
        .current_dir(scratch.cwd());
    cmd
}

/// Runs a command with stdin piped in, returning the collected output.
fn feed(cmd: &mut Command, stdin: &str) -> Output {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn termocab");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait for termocab")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A one-stage course: a single exercise whose answer is "a".
fn one_stage_course() -> String {
    r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Only", "level_hint": "beginner",
            "vocabulary": [
              { "word": "alpha", "translation": "a", "pos": "x", "example": "alpha" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "alpha", "answer": "a" }
            ] }
       ] }"#
        .to_string()
}

/// A tiny two-stage course with known answers: every exercise's answer
/// is "a" (stage 1) / "b" (stage 2). One exercise each keeps the 80%
/// bar simple: one correct answer completes a stage, zero does not.
fn two_stage_course() -> String {
    let stage = |id: u32, word: &str, answer: &str| {
        format!(
            r#"{{ "id": {id}, "title": "Stage {id}", "level_hint": "beginner",
                 "vocabulary": [
                   {{ "word": "{word}", "translation": "{answer}", "pos": "x", "example": "{word}" }}
                 ],
                 "exercises": [
                   {{ "type": "translation", "prompt": "{word}", "answer": "{answer}" }}
                 ] }}"#
        )
    };
    format!(
        r#"{{ "language": "test", "stages": [{}, {}] }}"#,
        stage(1, "alpha", "a"),
        stage(2, "beta", "b")
    )
}

/// Installs `two_stage_course` as `test` via add-lang into the scratch
/// $HOME's XDG data location — the real install path (item 5). Later
/// commands find it through $HOME alone, which is the whole point: an
/// installed binary must not depend on the current directory.
fn install_course(scratch: &Scratch) {
    let file = scratch.stages_file("course.json", &two_stage_course());
    let out = termocab(scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .expect("add-lang");
    assert!(out.status.success(), "add-lang failed: {}", stderr(&out));
}

#[test]
fn no_arguments_prints_usage_and_exits_zero() {
    let scratch = Scratch::new("usage");
    let out = termocab(&scratch).output().unwrap();
    assert!(out.status.success());
    assert!(stdout(&out).contains("usage: termocab <command>"));
}

#[test]
fn unknown_command_exits_two() {
    let scratch = Scratch::new("unknown");
    let out = termocab(&scratch).args(["frobnicate"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("unknown command: frobnicate"));
}

#[test]
fn extra_arguments_exit_two_with_the_usage_line() {
    let scratch = Scratch::new("arity");
    let out = termocab(&scratch)
        .args(["progress", "test", "junk"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("termocab progress <lang>"));
}

#[test]
fn traversal_in_language_name_exits_two() {
    let scratch = Scratch::new("traversal");
    let file = scratch.stages_file("x.json", &two_stage_course());
    let out = termocab(&scratch)
        .args(["add-lang", "../escaped", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("invalid language name"));
    assert!(!scratch.root.join("escaped").exists());
}

#[test]
fn fresh_progress_shows_every_stage_incomplete() {
    let scratch = Scratch::new("fresh");
    install_course(&scratch);
    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("0/2 stage(s) complete"));
    assert!(text.contains("[ ] Stage 1"));
    assert!(text.contains("next up: stage 1"));
    assert!(text.contains("streak: 0 day(s)"));
}

#[test]
fn passing_run_completes_the_stage_and_persists_it() {
    let scratch = Scratch::new("pass");
    install_course(&scratch);
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    let out = feed(&mut cmd, "a\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("stage complete: 1/1 correct"));
    assert!(stdout(&out).contains("progress saved."));

    // the progress file records the stage and a real calendar day
    let progress = scratch.home().join(".config/termocab/progress/test.txt");
    let text = std::fs::read_to_string(&progress).expect("progress file");
    assert!(text.contains("stage 1"));
    let day_ok = text.lines().any(|l| {
        l.starts_with("day ")
            && l.len() == 14
            && l.as_bytes()[8] == b'-'
            && l.as_bytes()[11] == b'-'
            && l[4..8].parse::<u32>().is_ok()
            && l[9..11].parse::<u32>().is_ok()
            && l[12..14].parse::<u32>().is_ok()
    });
    assert!(day_ok, "no valid `day YYYY-MM-DD` line in: {text}");

    // the next run knows the stage is done and offers stage 2
    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    assert!(stdout(&out).contains("1/2 stage(s) complete"));
    assert!(stdout(&out).contains("[x] Stage 1"));
    assert!(stdout(&out).contains("next up: stage 2"));
}

#[test]
fn failing_run_leaves_no_progress_file() {
    let scratch = Scratch::new("fail");
    install_course(&scratch);
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    let out = feed(&mut cmd, "wrong\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("below the 80% bar"));
    assert!(!stdout(&out).contains("progress saved."));
    assert!(!scratch
        .home()
        .join(".config/termocab/progress/test.txt")
        .exists());
}

#[test]
fn eof_run_writes_nothing() {
    let scratch = Scratch::new("eof");
    install_course(&scratch);
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    let out = feed(&mut cmd, "");
    assert!(out.status.success());
    assert!(stdout(&out).contains("input closed"));
    assert!(!scratch
        .home()
        .join(".config/termocab/progress/test.txt")
        .exists());
}

#[test]
fn stages_unlock_in_order() {
    let scratch = Scratch::new("order");
    install_course(&scratch);

    // skip-ahead is refused before anything is completed
    let out = termocab(&scratch)
        .args(["start", "test", "2"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("complete stage 1 first"));

    // stage 1 done: stage 2 is the next one, so it runs
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    feed(&mut cmd, "a\n");

    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test", "2"]);
    let out = feed(&mut cmd, "b\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("stage complete"));

    // and replaying the completed stage 1 is always allowed
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test", "1"]);
    let out = feed(&mut cmd, "a\n");
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn add_lang_refuses_overwrite_until_forced() {
    let scratch = Scratch::new("force");
    let file = scratch.stages_file("course.json", &two_stage_course());
    let installed = scratch
        .home()
        .join(".local/share/termocab/languages/test/stages.json");

    let run_add = |args: &[&str]| termocab(&scratch).args(args).output().unwrap();
    let file_arg = file.to_str().unwrap();

    let out = run_add(&["add-lang", "test", file_arg]);
    assert!(out.status.success());
    assert!(installed.exists());
    let out = run_add(&["add-lang", "test", file_arg]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("use --force"));
    let out = run_add(&["add-lang", "--force", "test", file_arg]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("replaced language"));
}

#[test]
fn invalid_content_is_rejected_and_installed_nowhere() {
    let scratch = Scratch::new("invalid");
    let bad = scratch.stages_file(
        "bad.json",
        r#"{ "language": "test", "stages": [
             { "id": 1, "title": "X", "level_hint": "b", "vocabulary": [],
               "exercises": [
                 { "type": "multiple_choice", "prompt": "p",
                   "options": ["x", "y"], "answer": "z" },
                 { "type": "fill_blank", "prompt": "no blank", "answer": "a" }
               ] }
           ] }"#,
    );
    let out = termocab(&scratch)
        .args(["add-lang", "test", bad.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("answer \"z\" is not one of the options"));
    assert!(err.contains("must contain \"___\""));
    assert!(!scratch
        .home()
        .join(".local/share/termocab/languages/test")
        .exists());
}

#[test]
fn check_reports_validation_verdicts() {
    let scratch = Scratch::new("check");
    let good = scratch.stages_file("good.json", &two_stage_course());
    let out = termocab(&scratch)
        .args(["check", good.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(stdout(&out).contains("ok: 2 stage(s)"));

    let bad = scratch.stages_file("bad.json", "{ not json");
    let out = termocab(&scratch)
        .args(["check", bad.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn installed_language_works_from_any_directory_with_only_home() {
    // Item 5's accept criterion: `cargo install` puts the binary
    // anywhere; with just $HOME set, add-lang installs into
    // ~/.local/share/termocab/languages and every command finds it
    // there — the current directory is irrelevant.
    let scratch = Scratch::new("homeonly");
    install_course(&scratch);
    assert!(scratch
        .home()
        .join(".local/share/termocab/languages/test/stages.json")
        .exists());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_termocab"));
    cmd.env_clear()
        .env("HOME", scratch.home())
        .current_dir("/") // not the repo, not the scratch dir
        .args(["start", "test"]);
    let out = feed(&mut cmd, "a\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("stage complete"));

    // XDG_DATA_HOME, when set, overrides the ~/.local/share location
    let scratch = Scratch::new("xdgdata");
    let file = scratch.stages_file("course.json", &two_stage_course());
    let out = termocab(&scratch)
        .env("XDG_DATA_HOME", scratch.root.join("data"))
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(scratch
        .root()
        .join("data/termocab/languages/test/stages.json")
        .exists());
}

#[test]
fn content_dir_override_beats_the_installed_location() {
    // First hit wins: TERMOCAB_CONTENT_DIR shadows the installed copy,
    // which lets a content author test a file without reinstalling.
    let scratch = Scratch::new("override");
    install_course(&scratch);

    let shadow = scratch.root.join("langs/test");
    std::fs::create_dir_all(&shadow).unwrap();
    std::fs::write(shadow.join("stages.json"), one_stage_course()).unwrap();

    let out = termocab(&scratch)
        .env("TERMOCAB_CONTENT_DIR", scratch.root.join("langs"))
        .args(["progress", "test"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    // the shadow (1 stage) wins over the installed course (2 stages)
    assert!(stdout(&out).contains("0/1 stage(s) complete"));
}

/// D16, the reported fresh-install failure: a built binary run from a
/// directory that holds no content — not the repo, not the data dir,
/// nothing installed — must still find the shipped courses compiled
/// into it. `termocab start french` was the README's first command and
/// it failed exactly here.
#[test]
fn builtin_courses_work_with_zero_setup() {
    let scratch = Scratch::new("builtin");
    // stage counts come from the course files themselves — the wave
    // convention (the count-agnostic rule of shipped_courses_validate)
    let course_stages = |lang: &str| -> usize {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("languages")
            .join(lang)
            .join("stages.json");
        let text = std::fs::read_to_string(&path).expect("course file");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("parse course");
        parsed["stages"].as_array().expect("stages").len()
    };
    // no add-lang, no TERMOCAB_CONTENT_DIR, empty cwd: only the binary
    for lang in [
        "french",
        "spanish",
        "german",
        "italian",
        "polish",
        "romanian",
        "portuguese",
        "swedish",
        "hungarian",
        "turkish",
        "icelandic",
        "catalan",
        "dutch",
    ] {
        let out = termocab(&scratch)
            .args(["progress", lang])
            .output()
            .unwrap();
        assert!(out.status.success(), "{lang}: {}", stderr(&out));
        let text = stdout(&out);
        assert!(
            text.contains(&format!("0/{} stage(s) complete", course_stages(lang))),
            "{lang}: {text}"
        );
        assert!(text.contains("[ ] Stage 1"), "{lang}: {text}");
        assert!(text.contains("next up: stage 1"), "{lang}: {text}");
    }
}

/// The exact command from the fresh-install report: `start french` from
/// an empty directory. The stage must open (content found via the
/// built-in), the closed stdin must abort the run, and the aborted run
/// must save nothing — the same EOF rules as any stage.
#[test]
fn start_on_a_builtin_course_runs_from_any_directory() {
    let scratch = Scratch::new("builtin-start");
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "french"]);
    let out = feed(&mut cmd, ""); // immediate EOF, like </dev/null
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("— Stage 1: Greetings and Basics —"), "{text}");
    assert!(
        text.contains("input closed — stopping this stage."),
        "{text}"
    );
    // aborted run, nothing written
    assert!(!scratch
        .home()
        .join(".config/termocab/progress/french.txt")
        .exists());
}

/// list-langs — the discovery verb: every available language, its size,
/// and the source it resolves from. Built-ins say so; installed files
/// show where they live; an installed file shadows the built-in course
/// of the same name (the D16 override story, end to end).
#[test]
fn list_langs_shows_builtins_installed_and_overrides() {
    let scratch = Scratch::new("listlangs");
    let row_for = |text: &str, lang: &str| -> String {
        text.lines()
            .find(|l| l.starts_with(lang))
            .unwrap_or_else(|| panic!("no {lang} row in:\n{text}"))
            .to_string()
    };

    // fresh: the shipped courses, all built-in, with real sizes
    // read from the same files `check` pins (count-agnostic)
    let out = termocab(&scratch).args(["list-langs"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    for lang in [
        "french",
        "spanish",
        "german",
        "italian",
        "polish",
        "romanian",
        "portuguese",
        "swedish",
        "hungarian",
        "turkish",
        "icelandic",
        "catalan",
        "dutch",
    ] {
        let stages = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("languages")
                .join(lang)
                .join("stages.json"),
        )
        .expect("course file");
        let parsed: serde_json::Value = serde_json::from_str(&stages).expect("parse course");
        let stage_count = parsed["stages"].as_array().expect("stages").len();
        let word_count: usize = parsed["stages"]
            .as_array()
            .expect("stages")
            .iter()
            .map(|s| s["vocabulary"].as_array().map_or(0, Vec::len))
            .sum();
        let row = row_for(&text, lang);
        assert!(row.ends_with("built-in"), "{lang}: {row}");
        assert!(
            row.contains(&format!("{stage_count} stage(s), {word_count} word(s)")),
            "{lang}: {row}"
        );
    }
    assert!(
        !text.contains("termocab/languages"),
        "nothing installed yet, no directory should show: {text}"
    );

    // an installed custom language shows its directory
    install_course(&scratch);
    let out = termocab(&scratch).args(["list-langs"]).output().unwrap();
    let text = stdout(&out);
    let installed_path = scratch
        .home()
        .join(".local/share/termocab/languages/test")
        .display()
        .to_string();
    let row = row_for(&text, "test");
    assert!(row.contains("2 stage(s), 2 word(s)"), "{row}");
    assert!(row.ends_with(&installed_path), "{row}");

    // installing a course over a built-in name: the note says so, the
    // listing flips to the installed copy, and the loader follows it
    let course = r#"{ "language": "french", "stages": [
         { "id": 1, "title": "Mini", "level_hint": "beginner",
            "vocabulary": [
              { "word": "bonjour", "translation": "hello", "pos": "x", "example": "Bonjour !" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "bonjour", "answer": "hello" }
            ] }
       ] }"#;
    let file = scratch.stages_file("mini-french.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "french", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("takes precedence over the built-in 'french' course"),
        "{}",
        stdout(&out)
    );

    let out = termocab(&scratch).args(["list-langs"]).output().unwrap();
    let text = stdout(&out);
    let row = row_for(&text, "french");
    assert!(row.contains("1 stage(s), 1 word(s)"), "{row}");
    assert!(
        row.ends_with(
            &scratch
                .home()
                .join(".local/share/termocab/languages/french")
                .display()
                .to_string()
        ),
        "{row}"
    );

    let out = termocab(&scratch)
        .args(["progress", "french"])
        .output()
        .unwrap();
    assert!(
        stdout(&out).contains("0/1 stage(s) complete"),
        "the installed copy wins over the built-in: {}",
        stdout(&out)
    );
}

/// The missing-language error is a directory of what to run instead:
/// the directories searched, everything available, and a suggestion
/// for the near-miss (D16's friendlier face).
#[test]
fn missing_language_error_lists_available_and_suggests() {
    let scratch = Scratch::new("missing");
    let out = termocab(&scratch)
        .args(["progress", "klingon"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("no content for language 'klingon'"), "{err}");
    assert!(err.contains("available languages:"), "{err}");
    assert!(err.contains("french"), "{err}");
    assert!(err.contains("termocab list-langs"), "{err}");
    assert!(!err.contains("did you mean"), "{err}");

    // the typo case suggests the fix, runnable as printed
    let out = termocab(&scratch)
        .args(["progress", "frensh"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("did you mean 'french'?"), "{err}");
    assert!(err.contains("termocab start french"), "{err}");
}

/// --version / -V print the package version; extra arguments are a
/// usage error, the same arity rule as `help`.
#[test]
fn version_flag_prints_the_package_version() {
    let scratch = Scratch::new("version");
    for flag in ["--version", "-V"] {
        let out = termocab(&scratch).args([flag]).output().unwrap();
        assert!(out.status.success(), "{flag}: {}", stderr(&out));
        assert_eq!(
            stdout(&out),
            format!("termocab {}\n", env!("CARGO_PKG_VERSION")),
            "{flag}"
        );
        let out = termocab(&scratch).args([flag, "junk"]).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{flag} takes no arguments");
    }
}

#[test]
fn review_flows_end_to_end() {
    let scratch = Scratch::new("review");
    install_course(&scratch);

    // complete stage 1 -> its vocabulary is due for review immediately
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    feed(&mut cmd, "a\n");

    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    assert!(stdout(&out).contains("reviews due today: 1 word(s)"));

    // review it correctly: the card moves up a box and the day counts
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "a\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("box 2"));
    assert!(stdout(&out).contains("review saved."));
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    assert!(std::fs::read_to_string(&srs)
        .expect("srs file")
        .contains("word alpha box 2"));

    // nothing is due anymore
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "");
    assert!(stdout(&out).contains("no cards due today"));
}

/// D8 end to end: a word whose translation is "hi" must accept the
/// listed alternative "hello" in a review — the reported salut case.
#[test]
fn review_accepts_listed_alternative_translations() {
    let scratch = Scratch::new("alt");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Only", "level_hint": "beginner",
            "vocabulary": [
              { "word": "salut", "translation": "hi",
                "accepted_translations": ["hello"],
                "pos": "interjection", "example": "Salut !" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "salut", "answer": "hi" }
            ] }
       ] }"#;
    let file = scratch.stages_file("alt.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // the listing surfaces the alternative
    let out = termocab(&scratch)
        .args(["list-words", "test"])
        .output()
        .unwrap();
    assert!(stdout(&out).contains("salut (interjection) — hi (also: hello)"));

    // complete the stage, then review with the ALTERNATIVE translation
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    feed(&mut cmd, "hi\n");

    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "hello\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("correct"), "{}", stdout(&out));
    assert!(!stdout(&out).contains("not quite"));
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    assert!(std::fs::read_to_string(&srs)
        .expect("srs file")
        .contains("word salut box 2"));
}

/// A three-exercise stage with distinct prompts and one shared answer,
/// so exercise order is observable while correctness is not.
fn three_prompt_course() -> String {
    r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Only", "level_hint": "beginner",
            "vocabulary": [
              { "word": "alpha", "translation": "a", "pos": "x", "example": "alpha" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "one", "answer": "a" },
              { "type": "translation", "prompt": "two", "answer": "a" },
              { "type": "translation", "prompt": "three", "answer": "a" }
            ] }
       ] }"#
        .to_string()
}

/// D7: same seed, same exercise order — `--seed N` is the reproducible
/// mode (and the mode tests can assert on). The second run is a replay
/// of the completed stage, which is always allowed.
#[test]
fn same_seed_replays_the_same_exercise_order() {
    let scratch = Scratch::new("seed");
    let file = scratch.stages_file("course.json", &three_prompt_course());
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    let run = || {
        let mut cmd = termocab(&scratch);
        cmd.args(["start", "--seed", "7", "test", "1"]);
        let out = feed(&mut cmd, "a\na\na\n");
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("stage complete"));
        stdout(&out)
            .lines()
            .filter(|l| l.contains("/3]"))
            .map(String::from)
            .collect::<Vec<_>>()
    };

    let first = run();
    let second = run();
    assert_eq!(first, second, "same seed must give the same order");
    assert_eq!(first.len(), 3, "all three exercises ran: {first:?}");
}

/// D7: --no-shuffle keeps the authored order, exercise for exercise.
#[test]
fn no_shuffle_keeps_the_authored_order() {
    let scratch = Scratch::new("noshuffle");
    let file = scratch.stages_file("course.json", &three_prompt_course());
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "a\na\na\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    let asked: Vec<&str> = text.lines().filter(|l| l.contains("/3]")).collect();
    assert_eq!(
        asked,
        vec!["[Only — 1/3] one", "[Only — 2/3] two", "[Only — 3/3] three"]
    );
}

/// D7: a bad flag combination is a usage error, exit 2 — not a
/// language-name lookup, and not a silent accept.
#[test]
fn start_rejects_bad_order_flags_as_usage_errors() {
    let scratch = Scratch::new("badflags");
    for args in [
        vec!["start", "test", "--shuffel"],
        vec!["start", "test", "--seed", "abc"],
        vec!["start", "test", "--seed"],
        vec!["start", "--no-shuffle", "--seed", "1", "test"],
    ] {
        let out = termocab(&scratch).args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&out).contains("termocab start"),
            "{args:?}: {}",
            stderr(&out)
        );
    }
}

/// Colors: a piped stdout is not a terminal, so feedback stays plain —
/// `termocab start french | tee log` must not spray escape codes into
/// the log. (The tty path and $NO_COLOR are covered by the unit test
/// of the same rule; a subprocess pipe can only see this side.)
#[test]
fn piped_output_stays_plain_even_with_color_available() {
    let scratch = Scratch::new("plainpipe");
    install_course(&scratch);

    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    let out = feed(&mut cmd, "a\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct."), "{}", text);
    assert!(
        !text.contains('\u{1b}'),
        "no ANSI escapes in a pipe: {text:?}"
    );

    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "a\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stdout(&out).contains('\u{1b}'));
}

/// stats <lang>: the learner's numbers as plain-text sparklines. The
/// fresh-learner shape (nothing scheduled) and the studied shape (one
/// stage complete: 1 card in box 1, due today, active today).
#[test]
fn stats_reports_numbers_and_sparklines() {
    let scratch = Scratch::new("stats");
    install_course(&scratch);

    // fresh learner: no cards yet, so the hint instead of charts
    let out = termocab(&scratch).args(["stats", "test"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("0/2 stage(s) complete, streak 0 day(s)"));
    assert!(text.contains("words: 2 in content, 0 scheduled, 0 due today"));
    assert!(text.contains("no cards scheduled yet"));

    // study: stage 1 done schedules alpha at box 1, due today
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "test"]);
    feed(&mut cmd, "a\n");

    let out = termocab(&scratch).args(["stats", "test"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("1/2 stage(s) complete, streak 1 day(s)"),
        "{text}"
    );
    assert!(
        text.contains("words: 2 in content, 1 scheduled, 1 due today"),
        "{text}"
    );
    // one card in box 1: the chart is a full block then five flats
    assert!(text.contains("boxes 1-6: █▁▁▁▁▁  (1 0 0 0 0 0)"), "{text}");
    // 14-day review load starting today
    assert!(
        text.contains("reviews due, next 14 days (today first): █▁▁▁▁▁▁▁▁▁▁▁▁▁"),
        "{text}"
    );
    // and the activity line, today at the right edge
    assert!(
        text.contains("active days, last 14 (oldest first): ▁▁▁▁▁▁▁▁▁▁▁▁▁█"),
        "{text}"
    );
    // plain text all the way
    assert!(!text.contains('\u{1b}'));
}

/// The reported crash: `termocab list-words french | head -2` used to
/// panic with "failed printing to stdout: Broken pipe". It must end the
/// way cat and git do — nothing on stderr, exit 141 (D6).
///
/// The course is deliberately far bigger than a pipe buffer (64 KiB),
/// so the child is guaranteed to still be writing when the read end
/// closes: the broken pipe is forced, not raced for.
#[test]
fn broken_pipe_is_silent_and_exits_141_like_cat_and_git() {
    let scratch = Scratch::new("epipe");

    // 4000 words -> ~250 KiB of list-words output, well past the buffer
    let mut vocabulary = String::new();
    let mut exercises = String::new();
    for i in 0..4000 {
        vocabulary.push_str(&format!(
            r#"{{ "word": "mot{i}", "translation": "t{i}", "pos": "x", "example": "mot{i}" }},"#
        ));
        if i == 0 {
            exercises.push_str(r#"{ "type": "translation", "prompt": "mot0", "answer": "t0" }"#);
        }
    }
    vocabulary.pop(); // trailing comma
    let course = format!(
        r#"{{ "language": "test", "stages": [
             {{ "id": 1, "title": "Big", "level_hint": "beginner",
                "vocabulary": [{vocabulary}], "exercises": [{exercises}] }}
           ] }}"#
    );
    let file = scratch.stages_file("big.json", &course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    let mut child = termocab(&scratch)
        .args(["list-words", "test"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn list-words");
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut first_line = String::new();
    reader.read_line(&mut first_line).expect("read a line");
    assert!(first_line.contains("Stage 1"));
    drop(reader); // close the pipe: `head` exiting, from the child's view

    let out = child.wait_with_output().expect("wait for list-words");
    assert_eq!(out.status.code(), Some(141), "exit code like cat and git");
    let err = stderr(&out);
    assert!(
        err.is_empty(),
        "a broken pipe is silent, stderr said: {err}"
    );
}

/// The shipped courses are the product: all must validate through the
/// real `check` verb — the same gate an outside author's file passes
/// before `add-lang` will install it. Spanish was the second language
/// (D9); German the third, its ß, ö, ü and capitalized nouns flowing
/// through content validation like everything else; Italian the
/// fourth, closing the big Western European set with its grave and
/// acute accents (perché, città, caffè) and its elision nouns; Polish
/// the fifth, the first engine change a language has forced since D9
/// (D10) — its whole Latin Extended-A set (ą ć ę ł ń ó ś ź ż) flowing
/// through content validation like everything else; Romanian the
/// sixth, the second extension (D11) — its five letters (ă â î ș ț)
/// and its enclitic-article and pot-infinitive drills flowing through
/// content validation like everything else; Portuguese the seventh,
/// the language D9 named when it widened the table — pure content,
/// zero engine changes, the tildes ã õ, the cedilla ç, the acute and
/// circumflex vowels and the crase à all flowing through content
/// validation like everything else; Swedish the eighth, the last clean
/// zero-change candidate inside the D9 table before Icelandic — its
/// å ä ö flowing through content validation like everything else;
/// Hungarian the ninth and Turkish the tenth (the double acutes ő ű
/// and the split ğ/ı question, D14 and D15); Icelandic the eleventh,
/// the zero-change payoff D9 was holding all along — its acutes and
/// its ö straight out of the Latin-1 table, its æ a ligature already
/// on file, its þ and ð closed exactly where D9 put them on day one;
/// Catalan the twelfth, the language D9 named on the same coverage
/// list — every accent already on file (à è é í ï ó ò ú ç), the one
/// new question its interpunct, answered by D18's fold: the punt
/// volat of "col·legi" rides the content through the same validation
/// gate as every other character; Dutch the thirteenth, the last
/// candidate the roadmap had named — every letter on file (é, the
/// trema ë of ideeën, all D9), the one new question its ligature,
/// answered by D19's fold: the Ĳ of "het Ĳsselmeer" rides the
/// content through the same validation gate as every other character.
#[test]
fn shipped_courses_validate() {
    let scratch = Scratch::new("shipped");
    for lang in [
        "french",
        "spanish",
        "german",
        "italian",
        "polish",
        "romanian",
        "portuguese",
        "swedish",
        "hungarian",
        "turkish",
        "icelandic",
        "catalan",
        "dutch",
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("languages")
            .join(lang)
            .join("stages.json");
        // the course files grow over time (the stages 6-10 wave, and
        // whatever comes after it), so the expected stage count is
        // read from the file itself: `check` must report exactly what
        // is on disk, and every shipped course must still validate
        let stages = std::fs::read_to_string(&path).expect("read course file");
        let parsed: serde_json::Value = serde_json::from_str(&stages).expect("parse course file");
        let expected = parsed["stages"].as_array().expect("stages array").len();
        let out = termocab(&scratch)
            .args(["check", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success(), "{lang}: {}", stderr(&out));
        assert!(
            stdout(&out).contains(&format!("ok: {expected} stage(s)")),
            "{lang}: {}",
            stdout(&out)
        );
    }
}

/// D9 end to end, in a Spanish course: a missing accent counts as
/// correct with the accented form as the hint ("si" for "sí",
/// "manana" for "mañana"), the inverted ¡ ¿ are edge punctuation
/// ("hola" is "¡Hola!" with no hint — nothing about the marks is
/// accent-like), and a review accepts the listed alternative
/// translation. The engine is language-agnostic, not French-agnostic.
#[test]
fn spanish_accents_and_inverted_marks_grade_end_to_end() {
    let scratch = Scratch::new("spanish");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Español", "level_hint": "beginner",
            "vocabulary": [
              { "word": "mañana", "translation": "tomorrow",
                "accepted_translations": ["morning"],
                "pos": "adverb", "example": "Nos vemos mañana." },
              { "word": "sí", "translation": "yes",
                "pos": "adverb", "example": "Sí, claro." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "mañana", "answer": "tomorrow" },
              { "type": "translation", "prompt": "yes", "answer": "sí" },
              { "type": "translation", "prompt": "hello", "answer": "¡Hola!" },
              { "type": "translation", "prompt": "¿Cómo estás?", "answer": "How are you?" },
              { "type": "fill_blank", "prompt": "Nos vemos ___.", "answer": "mañana" }
            ] }
       ] }"#;
    let file = scratch.stages_file("spanish.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right except the accents and the inverted
    // marks — all five must count toward the pass bar
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "tomorrow\nsi\nhola\nHow are you\nmanana\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: sí"), "{text}");
    assert!(text.contains("watch the accents: mañana"), "{text}");
    assert!(text.contains("5/5"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (mañana before sí), and the
    // accepted alternative "morning" counts for mañana
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "morning\nyes\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word mañana box 2"), "{srs}");
    assert!(srs.contains("word sí box 2"), "{srs}");
}

/// The third language end to end, in a German course. D9's table never
/// needed a change for it: a missing umlaut counts as correct with the
/// accented form as the hint ("schon" for "schön", "tschuss" for
/// "tschüss"), the sharp s opens exactly like a ligature ("gross" for
/// "groß", "heiss" for "heiß" — the hint shows the real spelling), and
/// the review accepts the listed alternative translation. The course is
/// German through and through: the answer to a noun is the noun with
/// its article, capitalization included, and grading is case-insensitive
/// on top of the accent fold.
#[test]
fn german_umlauts_and_sharp_s_grade_end_to_end() {
    let scratch = Scratch::new("german");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Deutsch", "level_hint": "beginner",
            "vocabulary": [
              { "word": "schön", "translation": "beautiful",
                "accepted_translations": ["pretty"],
                "pos": "adjective", "example": "Was für ein schöner Tag!" },
              { "word": "groß", "translation": "big",
                "pos": "adjective", "example": "Mein Bruder ist sehr groß." },
              { "word": "tschüss", "translation": "goodbye",
                "accepted_translations": ["bye"],
                "pos": "interjection", "example": "Tschüss, bis morgen!" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "beautiful", "answer": "schön" },
              { "type": "translation", "prompt": "big", "answer": "groß" },
              { "type": "translation", "prompt": "goodbye", "answer": "tschüss" },
              { "type": "fill_blank", "prompt": "Der Kaffee ist ___.", "answer": "heiß" },
              { "type": "translation", "prompt": "Wie heißt du?", "answer": "What's your name?" }
            ] }
       ] }"#;
    let file = scratch.stages_file("german.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the umlauts and the sharp s
    // stripped — all five must count toward the pass bar, each with the
    // properly spelled form as the hint
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "schon\ngross\ntschuss\nheiss\nWhat's your name\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: schön"), "{text}");
    assert!(text.contains("watch the accents: groß"), "{text}");
    assert!(text.contains("watch the accents: tschüss"), "{text}");
    assert!(text.contains("watch the accents: heiß"), "{text}");
    assert!(text.contains("5/5"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (groß, schön, tschüss), and
    // the accepted alternative "pretty" counts for schön, "bye" for
    // tschüss — the srs keys keep the accents and the sharp s verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "big\npretty\nbye\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word groß box 2"), "{srs}");
    assert!(srs.contains("word schön box 2"), "{srs}");
    assert!(srs.contains("word tschüss box 2"), "{srs}");
}

/// The fourth language end to end, in an Italian course — the last of
/// the big Western European languages the D9 table covers untouched.
/// A missing accent counts as correct with the accented form as the
/// hint ("si" for "sì", "perche" for "perché", "caffe" for "caffè" —
/// both the grave and the acute fold), the elision noun keeps its
/// article in the answer and in the srs key verbatim ("l'amico"), and
/// the modal drill wants the bare infinitive ("Voglio ___ un caffè" →
/// "bere"). The review accepts the listed alternative translations —
/// including "because" for perché, one word covering both directions
/// of the why/because duality.
#[test]
fn italian_accents_and_elisions_grade_end_to_end() {
    let scratch = Scratch::new("italian");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Italiano", "level_hint": "beginner",
            "vocabulary": [
              { "word": "ciao", "translation": "hello",
                "accepted_translations": ["hi", "bye"],
                "pos": "interjection", "example": "Ciao, come stai?" },
              { "word": "sì", "translation": "yes",
                "pos": "adverb", "example": "Sì, certo." },
              { "word": "perché", "translation": "why",
                "accepted_translations": ["because"],
                "pos": "question word", "example": "Perché no?" },
              { "word": "caffè", "translation": "coffee",
                "pos": "noun (masculine)", "example": "Un caffè, per favore." },
              { "word": "l'amico", "translation": "the friend",
                "pos": "noun (masculine)",
                "example": "Il mio amico si chiama Luca." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "ciao", "answer": "hello" },
              { "type": "translation", "prompt": "yes", "answer": "sì" },
              { "type": "translation", "prompt": "why", "answer": "perché" },
              { "type": "translation", "prompt": "the coffee", "answer": "caffè" },
              { "type": "translation", "prompt": "the friend", "answer": "l'amico" },
              { "type": "fill_blank", "prompt": "Voglio ___ un caffè.", "answer": "bere" }
            ] }
       ] }"#;
    let file = scratch.stages_file("italian.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the accents stripped — all
    // six must count toward the pass bar, each accent miss carrying
    // the properly spelled form as the hint
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "hello\nsi\nperche\ncaffe\nl'amico\nbere\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: sì"), "{text}");
    assert!(text.contains("watch the accents: perché"), "{text}");
    assert!(text.contains("watch the accents: caffè"), "{text}");
    assert!(text.contains("6/6"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (caffè, ciao, l'amico,
    // perché, sì), the accepted alternatives count — "hi" for ciao,
    // "because" for perché — and the srs keys keep the accents and
    // the elided article verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "coffee\nhi\nthe friend\nbecause\nyes\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word caffè box 2"), "{srs}");
    assert!(srs.contains("word ciao box 2"), "{srs}");
    assert!(srs.contains("word l'amico box 2"), "{srs}");
    assert!(srs.contains("word perché box 2"), "{srs}");
    assert!(srs.contains("word sì box 2"), "{srs}");
}

/// The fifth language end to end, in a Polish course — the first
/// engine change a language has forced since D9 (D10, the Latin
/// Extended-A extension). A missing ogonek, acute or dot-above counts
/// as correct with the accented form as the hint ("dziekuje" for
/// "dziękuję", "piatek" for "piątek"), and the stroke opens exactly
/// like the sharp s ("maly" for "mały", "zolty" for "żółty" — ż, ó
/// and ł folding in one word), while the srs keys keep the accented,
/// stroked forms verbatim. The want drill wants the bare infinitive
/// ("Chcę ___ kawę" → "pić"), and the review accepts the listed
/// alternative translations — "thanks" for dziękuję, "little" for
/// mały.
#[test]
fn polish_diacritics_and_the_stroke_grade_end_to_end() {
    let scratch = Scratch::new("polish");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Polski", "level_hint": "beginner",
            "vocabulary": [
              { "word": "dziękuję", "translation": "thank you",
                "accepted_translations": ["thanks"],
                "pos": "interjection", "example": "Dziękuję bardzo!" },
              { "word": "piątek", "translation": "Friday",
                "pos": "noun", "example": "W piątek idę do kina." },
              { "word": "żółty", "translation": "yellow",
                "pos": "adjective", "example": "Autobus jest żółty." },
              { "word": "mały", "translation": "small",
                "accepted_translations": ["little"],
                "pos": "adjective", "example": "Mieszkam w małym mieście." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "thank you", "answer": "dziękuję" },
              { "type": "translation", "prompt": "Friday", "answer": "piątek" },
              { "type": "translation", "prompt": "yellow", "answer": "żółty" },
              { "type": "translation", "prompt": "small", "answer": "mały" },
              { "type": "fill_blank", "prompt": "Chcę ___ kawę.", "answer": "pić" },
              { "type": "translation", "prompt": "Jak się nazywasz?",
                "answer": "What's your name?",
                "accepted_answers": ["What is your name?"] }
            ] }
       ] }"#;
    let file = scratch.stages_file("polish.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the diacritics and the
    // stroke stripped — all six must count toward the pass bar, each
    // miss carrying the properly spelled form as the hint
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(
        &mut cmd,
        "dziekuje\npiatek\nzolty\nmaly\npic\nWhat's your name\n",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: dziękuję"), "{text}");
    assert!(text.contains("watch the accents: piątek"), "{text}");
    assert!(text.contains("watch the accents: żółty"), "{text}");
    assert!(text.contains("watch the accents: mały"), "{text}");
    assert!(text.contains("watch the accents: pić"), "{text}");
    assert!(text.contains("6/6"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (dziękuję, mały, piątek,
    // żółty), the accepted alternatives count — "thanks" for
    // dziękuję, "little" for mały — and the srs keys keep the
    // diacritics and the stroke verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "thanks\nlittle\nFriday\nyellow\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word dziękuję box 2"), "{srs}");
    assert!(srs.contains("word mały box 2"), "{srs}");
    assert!(srs.contains("word piątek box 2"), "{srs}");
    assert!(srs.contains("word żółty box 2"), "{srs}");
}

/// The sixth language end to end, in a Romanian course — the second
/// engine change a language has forced (D11), and half of it upended
/// the D10 prediction: ș and ț decompose after all (s/t + comma
/// below), so they fold like any accent — "multumesc" for
/// "mulțumesc", "maine" for "mâine", "batran" for "bătrân" — while the
/// legacy cedilla spellings fold to the same words. The srs keys keep
/// the diacritics verbatim. The drills are Romanian through and
/// through: the enclitic definite article blanks at the END of the
/// noun ("Băiat___ citește ziarul." wants "ul"), and the modal drill
/// wants the bare infinitive after pot ("Putem ___ o pizza." →
/// "mânca"), with the a-prefixed dictionary infinitive counting too.
/// The review accepts the listed alternative translations — "thanks"
/// for mulțumesc.
#[test]
fn romanian_diacritics_and_the_enclitic_article_grade_end_to_end() {
    let scratch = Scratch::new("romanian");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Română", "level_hint": "beginner",
            "vocabulary": [
              { "word": "mulțumesc", "translation": "thank you",
                "accepted_translations": ["thanks"],
                "pos": "interjection", "example": "Mulțumesc foarte mult!" },
              { "word": "mâine", "translation": "tomorrow",
                "pos": "adverb", "example": "Mâine merg la școală." },
              { "word": "a mânca", "translation": "to eat",
                "pos": "verb", "example": "Vreau să mănânc o pizza." },
              { "word": "bătrân", "translation": "old",
                "pos": "adjective", "example": "Omul este bătrân." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "thank you", "answer": "mulțumesc" },
              { "type": "translation", "prompt": "tomorrow", "answer": "mâine" },
              { "type": "translation", "prompt": "to eat", "answer": "a mânca" },
              { "type": "translation", "prompt": "old", "answer": "bătrân" },
              { "type": "fill_blank", "prompt": "Băiat___ citește ziarul.", "answer": "ul" },
              { "type": "fill_blank", "prompt": "Putem ___ o pizza.", "answer": "mânca",
                "accepted_answers": ["a mânca"] }
            ] }
       ] }"#;
    let file = scratch.stages_file("romanian.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the diacritics stripped —
    // all six must count toward the pass bar, each miss carrying the
    // properly spelled form as the hint; the enclitic-article blank
    // needs no fold and grades plain correct
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "multumesc\nmaine\na manca\nbatran\nul\nmanca\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: mulțumesc"), "{text}");
    assert!(text.contains("watch the accents: mâine"), "{text}");
    assert!(text.contains("watch the accents: a mânca"), "{text}");
    assert!(text.contains("watch the accents: bătrân"), "{text}");
    assert!(text.contains("watch the accents: mânca"), "{text}");
    assert!(text.contains("6/6"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (a mânca, bătrân,
    // mulțumesc, mâine — byte order, so mulțumesc before mâine), the
    // accepted alternative "thanks" counts for mulțumesc, and the srs
    // keys keep the diacritics verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "to eat\nold\nthanks\ntomorrow\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word a mânca box 2"), "{srs}");
    assert!(srs.contains("word bătrân box 2"), "{srs}");
    assert!(srs.contains("word mâine box 2"), "{srs}");
    assert!(srs.contains("word mulțumesc box 2"), "{srs}");
}

/// The seventh language end to end, in a Portuguese course — the
/// language D9 named as its reason for widening the accent table to
/// the whole Latin-1 set, finally arriving, and the table needed
/// exactly nothing: a missing tilde, cedilla or circumflex counts as
/// correct with the accented form as the hint ("irmao" for "irmão",
/// "maca" for "maçã", "voce" for "você"), and the crase — the
/// preposition and the feminine article fused into one grave-accented
/// letter — folds like any accent ("a" for "à"), with the fused form
/// as the hint. The srs keys keep the diacritics verbatim. The want
/// drill wants the bare infinitive after quero ("Quero ___ uma
/// pizza." → "comer"), and the review accepts the listed alternative
/// translation — "thanks" for obrigado.
#[test]
fn portuguese_tildes_and_the_crase_grade_end_to_end() {
    let scratch = Scratch::new("portuguese");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Português", "level_hint": "beginner",
            "vocabulary": [
              { "word": "obrigado", "translation": "thank you",
                "accepted_translations": ["thanks"],
                "pos": "interjection", "example": "Obrigado pelo café!" },
              { "word": "irmão", "translation": "brother",
                "pos": "noun", "example": "Meu irmão fala português." },
              { "word": "maçã", "translation": "apple",
                "pos": "noun", "example": "Como uma maçã todos os dias." },
              { "word": "você", "translation": "you",
                "pos": "pronoun", "example": "Você fala português?" },
              { "word": "comer", "translation": "to eat",
                "pos": "verb", "example": "Quero comer uma pizza." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "thank you", "answer": "obrigado" },
              { "type": "translation", "prompt": "brother", "answer": "irmão" },
              { "type": "translation", "prompt": "apple", "answer": "maçã" },
              { "type": "translation", "prompt": "you", "answer": "você" },
              { "type": "translation", "prompt": "to eat", "answer": "comer" },
              { "type": "fill_blank", "prompt": "Vou ___ escola.", "answer": "à" },
              { "type": "fill_blank", "prompt": "Quero ___ uma pizza.", "answer": "comer" }
            ] }
       ] }"#;
    let file = scratch.stages_file("portuguese.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the tildes, the cedilla,
    // the circumflex and the crase's grave stripped — all seven must
    // count toward the pass bar, each miss carrying the properly
    // spelled form as the hint
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "obrigado\nirmao\nmaca\nvoce\ncomer\na\ncomer\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: irmão"), "{text}");
    assert!(text.contains("watch the accents: maçã"), "{text}");
    assert!(text.contains("watch the accents: você"), "{text}");
    assert!(text.contains("watch the accents: à"), "{text}");
    assert!(text.contains("7/7"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (comer, irmão, maçã,
    // obrigado, você), the accepted alternative "thanks" counts for
    // obrigado, and the srs keys keep the tildes, the cedilla and the
    // circumflex verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "to eat\nbrother\napple\nthanks\nyou\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word comer box 2"), "{srs}");
    assert!(srs.contains("word irmão box 2"), "{srs}");
    assert!(srs.contains("word maçã box 2"), "{srs}");
    assert!(srs.contains("word obrigado box 2"), "{srs}");
    assert!(srs.contains("word você box 2"), "{srs}");
}

/// The eighth language end to end, in a Swedish course — the last clean
/// zero-change candidate inside the D9 table, and the table needed
/// exactly nothing for it: a missing ring or umlaut counts as correct
/// with the accented form as the hint ("atta" for "åtta", "oga" for
/// "öga", "forsta" for "förstå" — and "gar" for "går"), while the srs
/// keys keep the diacritics verbatim. The drills are Swedish through and
/// through: the gender drill wants the indefinite article, en for common
/// nouns and ett for neuter ("___ barn sover." wants "ett"), the definite
/// article is enclitic like Romanian's and rides at the end of the noun
/// ("Flick___ läser boken." wants "an" — flicka → flickan), and the modal
/// drill wants the bare infinitive after vilja ("Jag vill ___ kaffe."
/// wants "dricka"). The review accepts the listed alternative translation
/// — "walks" for går.
#[test]
fn swedish_umlauts_and_the_enclitic_article_grade_end_to_end() {
    let scratch = Scratch::new("swedish");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Svenska", "level_hint": "beginner",
            "vocabulary": [
              { "word": "åtta", "translation": "eight",
                "pos": "number", "example": "Klockan är åtta." },
              { "word": "öga", "translation": "eye",
                "pos": "noun (neuter)", "example": "Hon har gröna ögon." },
              { "word": "äpple", "translation": "apple",
                "pos": "noun (neuter)", "example": "Jag äter ett äpple." },
              { "word": "går", "translation": "goes",
                "accepted_translations": ["walks"],
                "pos": "verb", "example": "Hon går till skolan." },
              { "word": "förstå", "translation": "understand",
                "pos": "verb", "example": "Jag förstår svenska." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "eight", "answer": "åtta" },
              { "type": "translation", "prompt": "eye", "answer": "öga" },
              { "type": "translation", "prompt": "apple", "answer": "äpple" },
              { "type": "translation", "prompt": "goes", "answer": "går" },
              { "type": "translation", "prompt": "understand", "answer": "förstå" },
              { "type": "fill_blank", "prompt": "___ barn sover.", "answer": "ett" },
              { "type": "fill_blank", "prompt": "Flick___ läser boken.", "answer": "an" },
              { "type": "fill_blank", "prompt": "Jag vill ___ kaffe.", "answer": "dricka" }
            ] }
       ] }"#;
    let file = scratch.stages_file("swedish.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the rings and the umlauts
    // stripped — all eight must count toward the pass bar, each miss
    // carrying the properly spelled form as the hint; the article and
    // suffix blanks need no fold and grade plain correct
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "atta\noga\napple\ngar\nforsta\nett\nan\ndricka\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: åtta"), "{text}");
    assert!(text.contains("watch the accents: öga"), "{text}");
    assert!(text.contains("watch the accents: äpple"), "{text}");
    assert!(text.contains("watch the accents: går"), "{text}");
    assert!(text.contains("watch the accents: förstå"), "{text}");
    assert!(text.contains("8/8"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (förstå, går, äpple, åtta,
    // öga — byte order), the accepted alternative "walks" counts for
    // går, and the srs keys keep the rings and the umlauts verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "understand\nwalks\napple\neight\neye\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word åtta box 2"), "{srs}");
    assert!(srs.contains("word äpple box 2"), "{srs}");
    assert!(srs.contains("word förstå box 2"), "{srs}");
    assert!(srs.contains("word går box 2"), "{srs}");
    assert!(srs.contains("word öga box 2"), "{srs}");
}

/// The ninth language end to end, in a Hungarian course. The third
/// engine extension a language has forced (D14), and the gentlest
/// one: the double-acute letters ő ű decompose like any accent, so
/// "no" counts with "nő" as the hint, "konnyu" with "könnyű", and
/// precomposed equals hand-decomposed with no hint at all. The course
/// is Hungarian through and through: the article drill is the a/az
/// alternation (a before consonants, az before vowels — fully
/// predictable from the noun's first letter, so the drill is the
/// alternation itself), the stage-3 signature is the plural suffix
/// blanked with its linking vowel — the linking vowel IS the vowel
/// harmony (ember___ wants ek, férfi___ wants ak, fiú___ wants bare
/// k) — and the modal drill is akarni/szeretni/tudni + the
/// infinitive, blanked at the end where Hungarian puts it. The review
/// keeps the double acutes verbatim in the srs keys.
#[test]
fn hungarian_double_acutes_grade_end_to_end() {
    let scratch = Scratch::new("hungarian");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Magyar", "level_hint": "beginner",
            "vocabulary": [
              { "word": "nő", "translation": "woman",
                "pos": "noun", "example": "A nő teát iszik." },
              { "word": "könnyű", "translation": "easy",
                "pos": "adjective", "example": "A gyakorlat könnyű." },
              { "word": "fű", "translation": "grass",
                "pos": "noun", "example": "A fű zöld." },
              { "word": "kettő", "translation": "two",
                "pos": "number", "example": "Egy, kettő, három!" },
              { "word": "főzni", "translation": "to cook",
                "accepted_translations": ["to make food"],
                "pos": "verb", "example": "Szeretek főzni." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "woman", "answer": "nő" },
              { "type": "translation", "prompt": "easy", "answer": "könnyű" },
              { "type": "translation", "prompt": "grass", "answer": "fű" },
              { "type": "translation", "prompt": "two", "answer": "kettő" },
              { "type": "translation", "prompt": "to cook", "answer": "főzni" },
              { "type": "fill_blank", "prompt": "___ férfi olvas.", "answer": "a" },
              { "type": "fill_blank", "prompt": "Az ember___ barátságosak.", "answer": "ek" },
              { "type": "fill_blank", "prompt": "Szeretek vacsorát ___.", "answer": "főzni" }
            ] }
       ] }"#;
    let file = scratch.stages_file("hungarian.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered right, but with the double acutes stripped
    // — all eight must count toward the pass bar, each miss carrying
    // the properly spelled form as the hint; the article and suffix
    // blanks need no fold and grade plain correct
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "no\nkonnyu\nfu\nketto\nfozni\na\nek\nfozni\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: nő"), "{text}");
    assert!(text.contains("watch the accents: könnyű"), "{text}");
    assert!(text.contains("watch the accents: fű"), "{text}");
    assert!(text.contains("watch the accents: kettő"), "{text}");
    assert!(text.contains("watch the accents: főzni"), "{text}");
    assert!(text.contains("8/8"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (byte order: főzni, fű,
    // kettő, könnyű, nő — ő U+0151 sorts before ű U+0171), the
    // accepted alternative "to make food" counts for főzni, and the
    // srs keys keep both double acutes verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "to cook\ngrass\ntwo\neasy\nwoman\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word nő box 2"), "{srs}");
    assert!(srs.contains("word kettő box 2"), "{srs}");
    assert!(srs.contains("word könnyű box 2"), "{srs}");
    assert!(srs.contains("word fű box 2"), "{srs}");
    assert!(srs.contains("word főzni box 2"), "{srs}");
}

/// The tenth language end to end, in a Turkish course. The fourth
/// engine extension a language has forced (D15), and the first to
/// split in two: the soft ğ decomposes like any accent (g + breve,
/// the very mark Romanian's ă carries), so "soguk" counts with
/// "soğuk" as the hint and "kardes" with "kardeş" through the D11
/// cedilla entry that turns out to be Turkish's own letter — while
/// dotless ı stays closed like ø: two different vowels, not one
/// word spelled twice, so "hayir" is wrong, never hinted, and the
/// stage still completes at 7/8 because a hinted fold counts toward
/// the bar and a closed miss does not. The course is Turkish
/// through and through: the stage-2 drill is bir (the invariant
/// indefinite article that doubles as the number one), the
/// harmony drill is the plural suffix itself (Kız___ wants lar,
/// the back vowels picking their own suffix), and the modal drill
/// is istemek + infinitive with -mek/-mak blanked mid-sentence
/// where SOV puts it (Kahve iç___ istiyorum wants mek). The
/// review keeps the Turkish letters verbatim in the srs keys.
#[test]
fn turkish_closed_letters_and_harmony_grade_end_to_end() {
    let scratch = Scratch::new("turkish");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Türkçe", "level_hint": "beginner",
            "vocabulary": [
              { "word": "soğuk", "translation": "cold",
                "pos": "adjective", "example": "Su soğuk." },
              { "word": "kardeş", "translation": "sibling",
                "accepted_translations": ["brother", "sister"],
                "pos": "noun", "example": "Kardeşim okulda." },
              { "word": "üç", "translation": "three",
                "pos": "number", "example": "Üç çocuk oynuyor." },
              { "word": "küçük", "translation": "small",
                "accepted_translations": ["little"],
                "pos": "adjective", "example": "Evim küçük." },
              { "word": "hayır", "translation": "no",
                "pos": "adverb", "example": "Hayır, istemiyorum." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "cold", "answer": "soğuk" },
              { "type": "translation", "prompt": "sibling", "answer": "kardeş",
                "accepted_answers": ["brother", "sister"] },
              { "type": "translation", "prompt": "three", "answer": "üç" },
              { "type": "translation", "prompt": "small", "answer": "küçük",
                "accepted_answers": ["little"] },
              { "type": "translation", "prompt": "no", "answer": "hayır" },
              { "type": "fill_blank", "prompt": "Bu ___ kız.", "answer": "bir" },
              { "type": "fill_blank", "prompt": "Kız___ oynuyor.", "answer": "lar" },
              { "type": "fill_blank", "prompt": "Kahve iç___ istiyorum.", "answer": "mek" }
            ] }
       ] }"#;
    let file = scratch.stages_file("turkish.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered with the accents stripped — the foldable
    // letters count with the properly spelled form as the hint, and
    // the one closed ı does not: "hayir" is wrong, never hinted,
    // so the run lands at 7/8 — still past the 80% bar, the stage
    // completes, and the hint counts toward it while the closed
    // miss does not. Both halves of D15 in one run.
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "soguk\nkardes\nuc\nkucuk\nhayir\nbir\nlar\nmek\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: soğuk"), "{text}");
    assert!(text.contains("watch the accents: kardeş"), "{text}");
    assert!(text.contains("watch the accents: üç"), "{text}");
    assert!(text.contains("watch the accents: küçük"), "{text}");
    assert!(text.contains("not quite — expected: hayır"), "{text}");
    assert!(!text.contains("watch the accents: hayır"), "{text}");
    assert!(text.contains("7/8"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (byte order: hayır,
    // kardeş, küçük, soğuk, üç), the accepted alternatives count
    // ("brother" for kardeş, "little" for küçük), and the srs keys
    // keep every Turkish letter verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "no\nbrother\nlittle\ncold\nthree\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word hayır box 2"), "{srs}");
    assert!(srs.contains("word kardeş box 2"), "{srs}");
    assert!(srs.contains("word küçük box 2"), "{srs}");
    assert!(srs.contains("word soğuk box 2"), "{srs}");
    assert!(srs.contains("word üç box 2"), "{srs}");
}

/// The eleventh language end to end, in an Icelandic course. The
/// roadmap called Icelandic the candidate that would collect the
/// closed letters the ı precedent predicted — and it collected with
/// zero engine changes: D9's table already held every letter the
/// language carries. The acutes and the ö fold like any accent
/// ("tolva" counts with "tölva" as the hint, "ja" with "já", and
/// "þu" with "þú" — ú folds, þ stays), the æ ligature opens exactly
/// where D9 put it beside œ and ß ("hae" counts with "hæ" as the
/// hint), and the closed pair answers the ı precedent itself: þ and
/// ð are letters in their own right, so "godur" is wrong for
/// "góður", never hinted, and the run lands at 7/8 — still past the
/// 80% bar, the hinted folds counting toward it and the closed miss
/// not. The course is Icelandic through and through: the stage-2
/// drill is the three-gender pronoun (hann/hún/það — Icelandic's
/// three noun genders surface through the pronouns, the
/// identification slot's edition), the stage-3 signature is the
/// enclitic definite article blanked where it lives, at the end of
/// the noun (Hestur___ wants urinn — hestur → hesturinn), and the
/// modal drill is vilja/þurfa + að + infinitive, the particle every
/// Icelandic infinitive carries. The review keeps the Icelandic
/// letters verbatim in the srs keys.
#[test]
fn icelandic_closed_letters_and_ligature_grade_end_to_end() {
    let scratch = Scratch::new("icelandic");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Íslenska", "level_hint": "beginner",
            "vocabulary": [
              { "word": "hæ", "translation": "hi",
                "accepted_translations": ["hello", "bye"],
                "pos": "interjection", "example": "Hæ, hvað segirðu?" },
              { "word": "þú", "translation": "you",
                "pos": "pronoun", "example": "Þú ert vinur minn." },
              { "word": "góður", "translation": "good",
                "pos": "adjective", "example": "Hann er góður." },
              { "word": "tölva", "translation": "computer",
                "pos": "noun", "example": "Tölvan er í herberginu." },
              { "word": "já", "translation": "yes",
                "pos": "adverb", "example": "Já, takk." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "hi", "answer": "hæ" },
              { "type": "translation", "prompt": "you", "answer": "þú" },
              { "type": "translation", "prompt": "good", "answer": "góður" },
              { "type": "translation", "prompt": "computer", "answer": "tölva" },
              { "type": "translation", "prompt": "yes", "answer": "já" },
              { "type": "fill_blank", "prompt": "Góð___ daginn!", "answer": "an" },
              { "type": "fill_blank", "prompt": "Hestur___ er fljótur.", "answer": "urinn" },
              { "type": "fill_blank", "prompt": "Þetta er tölva. ___ er góð.", "answer": "Hún" }
            ] }
       ] }"#;
    let file = scratch.stages_file("icelandic.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered with the foldable letters stripped — the
    // acutes, the ö and the æ ligature count with the properly
    // spelled form as the hint, and the closed ð does not: "godur"
    // is wrong for "góður", never hinted, so the run lands at 7/8 —
    // still past the 80% bar, the hinted folds counting toward it
    // and the closed miss not. The D9 payoff and the D15 precedent
    // in one run, with no engine change anywhere in sight.
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(&mut cmd, "hae\nþu\ngodur\ntolva\nja\nan\nurinn\nhun\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: hæ"), "{text}");
    assert!(text.contains("watch the accents: þú"), "{text}");
    assert!(text.contains("watch the accents: tölva"), "{text}");
    assert!(text.contains("watch the accents: já"), "{text}");
    assert!(text.contains("not quite — expected: góður"), "{text}");
    assert!(!text.contains("watch the accents: góður"), "{text}");
    assert!(text.contains("7/8"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (byte order: góður, hæ,
    // já, tölva, þú), the accepted alternatives count ("hello" for
    // hæ), and the srs keys keep every Icelandic letter verbatim —
    // including both members of the closed pair
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "good\nhello\nyes\ncomputer\nyou\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word hæ box 2"), "{srs}");
    assert!(srs.contains("word þú box 2"), "{srs}");
    assert!(srs.contains("word góður box 2"), "{srs}");
    assert!(srs.contains("word tölva box 2"), "{srs}");
    assert!(srs.contains("word já box 2"), "{srs}");
}

/// The twelfth language end to end, in a Catalan course. D18's answer
/// pinned three ways: the dotless spelling counts with the properly
/// spelled form as the hint ("pellicula" for "pel·lícula", the ł tier),
/// the ASCII hyphen some typists fall back to is a real miss
/// ("col-legi" is wrong for "col·legi", counting against the pass bar
/// exactly like the closed letters of Icelandic and Turkish), and the
/// accents around the dot fold like anywhere else ("gracies" for
/// "gràcies", "felic" for "feliç", "raim" for "raïm"). The review then
/// accepts every listed alternative translation — the homonyms this
/// course carries: "the college" for col·legi, "the time" for el
/// temps — and the srs keys keep the punt volat verbatim.
#[test]
fn catalan_interpunct_and_accents_grade_end_to_end() {
    let scratch = Scratch::new("catalan");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Català", "level_hint": "beginner",
            "vocabulary": [
              { "word": "gràcies", "translation": "thank you",
                "accepted_translations": ["thanks"],
                "pos": "interjection", "example": "Gràcies pel cafè!" },
              { "word": "feliç", "translation": "happy",
                "pos": "adjective", "example": "Sóc molt feliç." },
              { "word": "el raïm", "translation": "the grapes",
                "pos": "noun (masculine)", "example": "Mengem raïm." },
              { "word": "el col·legi", "translation": "the school",
                "accepted_translations": ["the college"],
                "pos": "noun (masculine)", "example": "Vaig al col·legi a peu." },
              { "word": "la pel·lícula", "translation": "the film",
                "accepted_translations": ["the movie"],
                "pos": "noun (feminine)", "example": "La pel·lícula comença a les vuit." },
              { "word": "el temps", "translation": "the weather",
                "accepted_translations": ["the time"],
                "pos": "noun (masculine)", "example": "Quin temps fa avui?" }
            ],
            "exercises": [
              { "type": "translation", "prompt": "thank you", "answer": "gràcies" },
              { "type": "translation", "prompt": "happy", "answer": "feliç" },
              { "type": "translation", "prompt": "the grapes", "answer": "el raïm" },
              { "type": "translation", "prompt": "the school", "answer": "el col·legi" },
              { "type": "translation", "prompt": "the film", "answer": "la pel·lícula" },
              { "type": "translation", "prompt": "the weather", "answer": "el temps" },
              { "type": "fill_blank", "prompt": "Vaig ___ col·legi a peu.", "answer": "al" },
              { "type": "fill_blank", "prompt": "Quin ___ fa avui?", "answer": "temps" },
              { "type": "fill_blank", "prompt": "Ahir vaig ___ pizza.", "answer": "menjar" }
            ] }
       ] }"#;
    let file = scratch.stages_file("catalan.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered with the marks stripped — the accents and the
    // interpunct count with the properly spelled form as the hint, but
    // the hyphen fallback does not: "el col-legi" is wrong for
    // "el col·legi", never hinted, so the run lands at 8/9 — still past
    // the 80% bar, the hinted folds counting toward it and the hyphen
    // miss not. The D18 answer in one run.
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(
        &mut cmd,
        "gracies\nfelic\nel raim\nel col-legi\nla pellicula\nel temps\nal\ntemps\nmenjar\n",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: gràcies"), "{text}");
    assert!(text.contains("watch the accents: feliç"), "{text}");
    assert!(text.contains("watch the accents: el raïm"), "{text}");
    assert!(text.contains("watch the accents: la pel·lícula"), "{text}");
    assert!(text.contains("not quite — expected: el col·legi"), "{text}");
    assert!(!text.contains("watch the accents: el col·legi"), "{text}");
    assert!(text.contains("8/9"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (byte order: el col·legi,
    // el raïm, el temps, feliç, gràcies, la pel·lícula), the accepted
    // alternatives all count — "the college", "the time", "the movie",
    // "thanks" — and the srs keys keep the punt volat verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(
        &mut cmd,
        "the college\nthe grapes\nthe time\nhappy\nthanks\nthe movie\n",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word el col·legi box 2"), "{srs}");
    assert!(srs.contains("word la pel·lícula box 2"), "{srs}");
    assert!(srs.contains("word el raïm box 2"), "{srs}");
    assert!(srs.contains("word el temps box 2"), "{srs}");
    assert!(srs.contains("word feliç box 2"), "{srs}");
    assert!(srs.contains("word gràcies box 2"), "{srs}");
}

/// The thirteenth language end to end, in a Dutch course. D19's answer
/// pinned three ways: the two-letter ij every modern text types counts
/// with the ligature form as the hint ("het IJsselmeer" for "het
/// Ĳsselmeer", the œ/æ/ß tier), the ligature typed back folds to the
/// two letters it spells with no invented matches anywhere, and the y
/// of the schoolbook shorthand is no spelling of the word ("myn" is
/// wrong for "mijn", counting against the pass bar exactly like the
/// closed letters of Icelandic, Turkish and the hyphen of Catalan).
/// Around the ligature, the accents ride D9 untouched: "een" counts
/// with "één" — the one Dutch word where the accent does grammar,
/// separating the number from the article. The review then keeps the
/// srs keys verbatim: het Ĳsselmeer and één, ligature and accent
/// riding the keys untouched.
#[test]
fn dutch_ligature_and_ij_grade_end_to_end() {
    let scratch = Scratch::new("dutch");
    let course = r#"{ "language": "test", "stages": [
         { "id": 1, "title": "Nederlands", "level_hint": "beginner",
            "vocabulary": [
              { "word": "één", "translation": "one",
                "accepted_translations": ["1"],
                "pos": "number", "example": "Ik wil één koffie." },
              { "word": "mijn", "translation": "my",
                "pos": "pronoun", "example": "Mijn huis is klein." },
              { "word": "tijd", "translation": "time",
                "pos": "noun", "example": "Ik heb geen tijd." },
              { "word": "de fiets", "translation": "the bicycle",
                "accepted_translations": ["the bike"],
                "pos": "noun (de)", "example": "Ik fiets naar mijn werk." },
              { "word": "het meisje", "translation": "the girl",
                "pos": "noun (het)", "example": "Het meisje leest een boek." },
              { "word": "het Ĳsselmeer", "translation": "the IJsselmeer",
                "pos": "noun (het)", "example": "We varen over het Ĳsselmeer." }
            ],
            "exercises": [
              { "type": "translation", "prompt": "one", "answer": "één",
                "accepted_answers": ["een"] },
              { "type": "translation", "prompt": "my", "answer": "mijn" },
              { "type": "translation", "prompt": "the IJsselmeer", "answer": "het Ĳsselmeer" },
              { "type": "translation", "prompt": "time", "answer": "tijd" },
              { "type": "translation", "prompt": "the bicycle", "answer": "de fiets" },
              { "type": "translation", "prompt": "the girl", "answer": "het meisje" },
              { "type": "fill_blank", "prompt": "___, twee, drie.", "answer": "één" },
              { "type": "fill_blank", "prompt": "Ik heb geen ___.", "answer": "tijd" },
              { "type": "fill_blank", "prompt": "___ fiets is rood.", "answer": "Mijn" }
            ] }
       ] }"#;
    let file = scratch.stages_file("dutch.json", course);
    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // everything answered the way a learner without a ligature key
    // types — the two-letter ij — except "myn", the schoolbook y that
    // is no spelling of mijn: the ligature folds count with the
    // properly spelled forms as the hints, the y miss counts against
    // the pass bar, and the run lands at 8/9 — still past the 80%
    // bar. The D19 answer in one run.
    let mut cmd = termocab(&scratch);
    cmd.args(["start", "--no-shuffle", "test"]);
    let out = feed(
        &mut cmd,
        "een\nmyn\nhet IJsselmeer\ntijd\nde fiets\nhet meisje\néen\ntijd\nmijn\n",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("watch the accents: één"), "{text}");
    assert!(text.contains("watch the accents: het Ĳsselmeer"), "{text}");
    assert!(text.contains("not quite — expected: mijn"), "{text}");
    assert!(!text.contains("watch the accents: mijn"), "{text}");
    assert!(text.contains("8/9"), "{text}");
    assert!(text.contains("stage complete"), "{text}");

    // review: due cards come alphabetical (byte order: de fiets, het
    // meisje, het Ĳsselmeer, mijn, tijd, één), and the srs keys keep
    // the ligature and the accent verbatim
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(
        &mut cmd,
        "the bicycle\nthe girl\nthe IJsselmeer\nmy\ntime\none\n",
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("correct"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    assert!(srs.contains("word de fiets box 2"), "{srs}");
    assert!(srs.contains("word het meisje box 2"), "{srs}");
    assert!(srs.contains("word het Ĳsselmeer box 2"), "{srs}");
    assert!(srs.contains("word mijn box 2"), "{srs}");
    assert!(srs.contains("word tijd box 2"), "{srs}");
    assert!(srs.contains("word één box 2"), "{srs}");
}

/// The stages 6-10 wave end to end. A course that ships ten stages
/// keeps the unlock ladder working past five (stage 6 stays locked
/// until stage 5 completes, exactly like stage 2 after stage 1),
/// the elementary level_hint the wave introduced shows in `progress`
/// where beginner always did, a stage past the file's end is the
/// "no stage" runtime error it has always been, and — the point of
/// the whole wave — every newly completed stage's vocabulary joins
/// the review pool automatically, stage by stage.
#[test]
fn stages_six_to_ten_unlock_review_and_report_their_level() {
    let scratch = Scratch::new("wave");

    // ten tiny stages: one word and one fill-blank each, the first
    // five beginner like every shipped stage before the wave, the
    // last five elementary — the field the wave was built to use
    let stages: Vec<String> = (1..=10)
        .map(|n| {
            let level = if n <= 5 { "beginner" } else { "elementary" };
            format!(
                r#"{{ "id": {n}, "title": "Stage {n}", "level_hint": "{level}",
                     "vocabulary": [
                       {{ "word": "mot{n}", "translation": "word{n}",
                          "pos": "noun", "example": "Mot {n}." }}
                     ],
                     "exercises": [
                       {{ "type": "fill_blank", "prompt": "___ is stage {n}.",
                          "answer": "mot{n}" }}
                     ] }}"#
            )
        })
        .collect();
    let course = format!(
        r#"{{ "language": "test", "stages": [{}] }}"#,
        stages.join(", ")
    );
    let file = scratch.stages_file("wave.json", &course);

    let out = termocab(&scratch)
        .args(["check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("ok: 10 stage(s)"), "{}", stdout(&out));

    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // the ladder still climbs in order: stage 6 is locked before
    // stage 1 completes, with the same message stage 2 always got
    let out = termocab(&scratch)
        .args(["start", "test", "6"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("complete stage 1 first"));

    // progress shows all ten stages with the elementary hint where
    // the wave's stages live
    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    let text = stdout(&out);
    assert!(text.contains("0/10 stage(s) complete"), "{text}");
    assert!(
        text.contains("[ ] Stage 6 — Stage 6 [elementary]"),
        "{text}"
    );
    assert!(
        text.contains("[ ] Stage 10 — Stage 10 [elementary]"),
        "{text}"
    );
    assert!(text.contains("[ ] Stage 5 — Stage 5 [beginner]"), "{text}");

    // climbing the ladder: stages 1 through 6, one answer each
    for n in 1..=6 {
        let mut cmd = termocab(&scratch);
        cmd.args(["start", "--no-shuffle", "test", &n.to_string()]);
        let out = feed(&mut cmd, &format!("mot{n}\n"));
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("1/1"), "stage {n}: {}", stdout(&out));
    }

    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    let text = stdout(&out);
    assert!(text.contains("6/10 stage(s) complete"), "{text}");
    assert!(
        text.contains("[x] Stage 6 — Stage 6 [elementary]"),
        "{text}"
    );
    assert!(
        text.contains("[ ] Stage 7 — Stage 7 [elementary]"),
        "{text}"
    );

    // a stage past the file's end stays the runtime error it has
    // always been — the wave grew the courses, not the sky
    let out = termocab(&scratch)
        .args(["start", "test", "11"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("no stage 11 in test"));

    // the point of the wave: completed stages' vocabulary feeds the
    // review pool automatically — exactly six cards, the words of
    // stages 1-6, none from the locked 7-10
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    let out = feed(&mut cmd, "word1\nword2\nword3\nword4\nword5\nword6\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("6/6"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    for n in 1..=6 {
        assert!(srs.contains(&format!("word mot{n} box 2")), "{srs}");
    }
    for n in 7..=10 {
        assert!(!srs.contains(&format!("word mot{n} ")), "{srs}");
    }
}

/// The stages 11-15 wave end to end. Fifteen stages exercise the whole
/// ladder the wave grew: the intermediate level_hint shows in `progress`
/// beside the two older bands, the unlock ladder keeps climbing past ten
/// exactly like it did past five, a stage past the file's end stays the
/// "no stage" runtime error, and every completed stage's vocabulary keeps
/// feeding the review pool stage by stage — now eleven cards deep — while
/// locked stages 12-15 stay out of it.
#[test]
fn stages_eleven_to_fifteen_unlock_review_and_report_their_level() {
    let scratch = Scratch::new("wave15");

    // fifteen tiny stages: one word and one fill-blank each, five
    // beginner, five elementary, five intermediate — the field the
    // stages 11-15 wave put to work
    let stages: Vec<String> = (1..=15)
        .map(|n| {
            let level = if n <= 5 {
                "beginner"
            } else if n <= 10 {
                "elementary"
            } else {
                "intermediate"
            };
            format!(
                r#"{{ "id": {n}, "title": "Stage {n}", "level_hint": "{level}",
                     "vocabulary": [
                       {{ "word": "mot{n}", "translation": "word{n}",
                          "pos": "noun", "example": "Mot {n}." }}
                     ],
                     "exercises": [
                       {{ "type": "fill_blank", "prompt": "___ is stage {n}.",
                          "answer": "mot{n}" }}
                     ] }}"#
            )
        })
        .collect();
    let course = format!(
        r#"{{ "language": "test", "stages": [{}] }}"#,
        stages.join(", ")
    );
    let file = scratch.stages_file("wave15.json", &course);

    let out = termocab(&scratch)
        .args(["check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("ok: 15 stage(s)"), "{}", stdout(&out));

    let out = termocab(&scratch)
        .args(["add-lang", "test", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // the ladder climbs in order across all three bands: stage 11 is
    // locked before stage 1 completes, with the same message stage 2
    // always got
    let out = termocab(&scratch)
        .args(["start", "test", "11"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("complete stage 1 first"));

    // progress shows all fifteen stages, each band's hint where its
    // stages live
    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    let text = stdout(&out);
    assert!(text.contains("0/15 stage(s) complete"), "{text}");
    assert!(text.contains("[ ] Stage 1 — Stage 1 [beginner]"), "{text}");
    assert!(
        text.contains("[ ] Stage 6 — Stage 6 [elementary]"),
        "{text}"
    );
    assert!(
        text.contains("[ ] Stage 11 — Stage 11 [intermediate]"),
        "{text}"
    );
    assert!(
        text.contains("[ ] Stage 15 — Stage 15 [intermediate]"),
        "{text}"
    );

    // climbing the ladder: stages 1 through 11, one answer each — the
    // stage-10 boundary the previous wave ended at is not special
    for n in 1..=11 {
        let mut cmd = termocab(&scratch);
        cmd.args(["start", "--no-shuffle", "test", &n.to_string()]);
        let out = feed(&mut cmd, &format!("mot{n}\n"));
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("1/1"), "stage {n}: {}", stdout(&out));
    }

    let out = termocab(&scratch)
        .args(["progress", "test"])
        .output()
        .unwrap();
    let text = stdout(&out);
    assert!(text.contains("11/15 stage(s) complete"), "{text}");
    assert!(
        text.contains("[x] Stage 11 — Stage 11 [intermediate]"),
        "{text}"
    );
    assert!(
        text.contains("[ ] Stage 12 — Stage 12 [intermediate]"),
        "{text}"
    );

    // a stage past the file's end stays the runtime error it has
    // always been — the wave grew the courses, not the sky
    let out = termocab(&scratch)
        .args(["start", "test", "16"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("no stage 16 in test"));

    // the review pool grows with every completed stage: exactly eleven
    // cards, the words of stages 1-11, none from the locked 12-15
    let mut cmd = termocab(&scratch);
    cmd.args(["review", "test"]);
    // reviews present due cards in alphabetical word order, and
    // alphabetically mot10 and mot11 sort before mot2
    let order = [1, 10, 11, 2, 3, 4, 5, 6, 7, 8, 9];
    let answers: String = order.iter().map(|n| format!("word{n}\n")).collect();
    let out = feed(&mut cmd, &answers);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("11/11"), "{text}");
    assert!(!text.contains("not quite"), "{text}");
    let srs = scratch.home().join(".config/termocab/srs/test.txt");
    let srs = std::fs::read_to_string(&srs).expect("srs file");
    for n in 1..=11 {
        assert!(srs.contains(&format!("word mot{n} box 2")), "{srs}");
    }
    for n in 12..=15 {
        assert!(!srs.contains(&format!("word mot{n} ")), "{srs}");
    }
}
