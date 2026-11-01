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
/// acute accents (perché, città, caffè) and its elision nouns.
#[test]
fn shipped_courses_validate() {
    let scratch = Scratch::new("shipped");
    for lang in ["french", "spanish", "german", "italian"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("languages")
            .join(lang)
            .join("stages.json");
        let out = termocab(&scratch)
            .args(["check", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success(), "{lang}: {}", stderr(&out));
        assert!(
            stdout(&out).contains("ok: 5 stage(s)"),
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
