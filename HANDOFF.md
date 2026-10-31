# HANDOFF — termocab

> This document is written for an AI coding agent taking over full development of **termocab**. It consolidates every decision made during the planning/bootstrap phase (Aug 2026), the MVP implementation session (Oct 2026), the review-fix session (Oct 2026), and the usage-review session (Oct 2026) so no prior conversation context is needed to continue the project. **The repository itself is the source of truth** — if anything here conflicts with what you find in the actual code, trust the code, and update this file to match.

## 1. What this is

termocab is an open-source, terminal-native language-learning CLI. Tagline: **"Language learning for terminal enthusiasts."**

Non-negotiable philosophy:
- Runs entirely in the terminal. No GUI, no browser, no server, no network dependency. Fully offline, fully local.
- Fast, lightweight, keyboard-driven.
- No ads, no mandatory account, respects user data.
- UX modeled on real Unix tools — **git** is the explicit style reference: clear, direct subcommands. No interactive menus, no numbered prompts waiting for input. Every command is a complete, self-contained verb: `termocab <subcommand> [args]`.

Name history: `ling` → `vex-lang` → `tvocab` → **`termocab`** (current, final). An earlier, broader `vex-lang` vision doc described a full TUI + detailed SRS system — that is a long-term reference only, **not** current MVP scope.

## 2. Current status

The MVP described in §8 is **implemented and verified**: engine grading, progress tracking, plain-text storage, session runner, and the full CLI all work end-to-end.

The **SRS layer (§8a) is implemented and verified** (Oct 2026): Leitner-box scheduling, plain-text persistence, a `review` verb, and a due-count line in `progress`.

The **review-fix session (Oct 2026)** hardened everything: content validation at load time, the `LangId` newtype (no path traversal), exact CLI arities, XDG-based content installation, real local time from the system timezone, forgiving grading with `accepted_answers`, enforced stage ordering, `--force` for `add-lang`, a `check` verb, CI, MIT license, README — and replaced the sample fixture with a real five-stage French course (73 words, 177 exercises).

The **usage-review session (Oct 2026)** closed the loop on real usage: broken pipes end quietly like cat and git (D6), reviews accept alternative translations via `accepted_translations` (D8), stage exercises shuffle by default with `--no-shuffle`/`--seed <n>` (D7), feedback turns green/red on a terminal behind `$NO_COLOR`, and a `stats` verb charts boxes, review load and activity as plain-text sparklines.

```
termocab/
├── Cargo.toml              # deps: serde (derive), serde_json — nothing else
├── Cargo.lock
├── .github/workflows/ci.yml  # fmt + clippy -D warnings + test + release build, ubuntu & macos
├── .gitignore
├── LICENSE                 # MIT
├── README.md
├── languages/
│   └── french/
│       └── stages.json     # real course: 5 stages, 73 words, 177 exercises
├── tests/
│   └── cli.rs              # binary-level integration tests (subprocess, hermetic env)
└── src/
    ├── main.rs             # thin entry: collects args, calls cli::run, prints CliError
    ├── lang.rs             # LangId newtype — the only way a language name reaches a path
    ├── engine/
    │   └── mod.rs          # domain types + grading (normalization, Grade) + 80% rule
    ├── content/
    │   └── mod.rs          # JSON wire types → engine types + content validation
    ├── progress/
    │   └── mod.rs          # LanguageProgress domain type, streak logic, tests
    ├── srs/
    │   └── mod.rs          # Leitner scheduling domain: boxes, due dates, tests
    ├── storage/
    │   └── mod.rs          # plain-text persistence for progress AND srs state
    ├── session/
    │   └── mod.rs          # stage runner + review runner behind a Terminal trait
    ├── cli/
    │   ├── mod.rs          # subcommand dispatch, Clock, tests
    │   ├── time.rs         # "what day is it": owned TZif reader, civil-date math
    │   └── testdata/       # committed TZif fixtures (utc, beirut) for the tests
    └── testenv.rs          # #[cfg(test)]-only: shared env lock + RAII temp dirs
```

Verified: `cargo build` warning-free, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` green (93 unit + 22 binary-level integration tests), and a scripted playthrough of the full French course passes end to end (5/5 stages, streak, replay, 73-card review). CI (§11) runs all of it on ubuntu and macOS.

## 3. Architecture — read this before writing any code

Strict layered separation:

```
Terminal/CLI  →  Application Logic  →  Learning Engine  →  Progress Tracker  →  Storage Layer
                                    ↘ SRS Scheduler ↗
```

**The single most important rule in this codebase:** `engine/`, `progress/` and `srs/` must never know how anything is stored. No JSON, no file I/O, no SQLite — ever. They only work with plain in-memory types. `content/` and `storage/` are the only modules allowed to know both about the domain types and about a storage format at the same time; they convert between the two. This is what lets the project move from JSON/plain-text to SQLite later by rewriting only the storage-facing modules.

`session/` is the application layer: it knows `engine::`, `progress::` and `srs::` but not files or dates. `cli/` is the only module that touches `std::env`, exit codes, and "what day is it" (the storage layer takes the config root as a parameter; `cli/` resolves `$XDG_CONFIG_HOME`/`$HOME` and hands it in). The `Terminal` trait in `session/` abstracts stdin/stdout so tests drive the runner with scripted answers, and `read_line` returns an `Input` enum so EOF and unreadable input are distinct.

`src/lang.rs` is a deliberate leaf addition to this picture: a tiny shared `LangId` newtype (validated `[a-z0-9_-]{1,32}`) that `cli/`, `content/` and `storage/` all use so an unvalidated language name can never reach a file path. It depends on nothing, so it adds no coupling between layers.

`srs/` is deliberately the same kind of module as `progress/` — a pure domain layer sitting beside it, not under it: it never reads `LanguageProgress`, it just receives "these words, this date" from the caller. Which words qualify (vocabulary of completed stages) is decided in `cli/`, where content and progress meet.

This separation is **not up for debate** — it was decided deliberately and reaffirmed repeatedly. Everything else about the architecture (exact module boundaries beyond this) can be proposed and changed if there's a good reason.

## 4. Domain model (in `engine/mod.rs`)

```rust
pub struct Stage {
    pub id: u32,
    pub title: String,
    pub level_hint: String,
    pub vocabulary: Vec<Vocabulary>,
    pub exercises: Vec<Exercise>,
}

pub struct Vocabulary {
    pub word: String,
    pub translation: String,
    /// Alternative correct translations for review — "salut" is "hi"
    /// but "hello" must not be wrong (D8). Same pattern as
    /// accepted_answers on exercises.
    pub accepted_translations: Vec<String>,
    pub pos: String,
    pub example: String,
}

pub enum Exercise {
    Translation { prompt: String, answer: String, accepted_answers: Vec<String> },
    MultipleChoice { prompt: String, options: Vec<String>, answer: String },
    FillBlank { prompt: String, answer: String, accepted_answers: Vec<String> },
}

pub enum Grade { Correct, CorrectWithHint(String), Incorrect }
```

Engine API: `Exercise::prompt()`, `Exercise::answer()`, `Exercise::accepted_answers()`, `Exercise::grade(attempt) -> Grade` and `Exercise::is_correct(attempt)`; `Stage::is_completed_by(correct_count)` implementing the 80% rule with integer math via `STAGE_PASS_PERCENT`.

Grading (settled in the review-fix session, decision D1): both sides go through `normalize_strict` — trim, lowercase, collapse internal whitespace, strip leading/trailing punctuation (`. ? ! , ;`), fold apostrophes (`’` → `'`), and fold Unicode composition via a hand-written 16-entry French accent table (àâäçéèêëîïôöùûüÿ decompose to base + combining mark, so precomposed `é` equals macOS's `e` + U+0301). A second, loose pass drops the combining marks and opens the œ/æ ligatures: an accent-only mismatch is **correct**, and `grade` returns `CorrectWithHint` carrying the properly accented form, which the session prints as `correct — watch the accents: étudiant`. `accepted_answers` (optional field on translation and fill_blank, absent in old files) lists alternative correct spellings — contractions ("I'm a student"), gendered forms ("Je suis étudiante").

No `#[derive(Serialize/Deserialize)]` here, deliberately — that would leak storage-awareness into `engine/`. The JSON-shaped mirror types live in `content/mod.rs` and get converted into these via `.into_engine()`.

## 4a. SRS domain model (in `srs/mod.rs`)

```rust
pub struct SrsState { /* BTreeMap<word, Card> — word order stable, no dupes */ }
pub struct Card    { pub box_number: u8 /* 1..=6 */, pub due: String /* YYYY-MM-DD */ }
pub struct Schedule { pub box_number: u8, pub due: String }  // returned by answer()
```

Rules (classic Leitner, all in `srs/`, all pure):
- Six boxes, intervals `BOX_INTERVALS_DAYS = [1, 2, 4, 8, 16, 32]`.
- Correct answer: promote one box (capped at 6). Wrong answer: demote to box 1.
- Next due = `date + interval(new box)` — the NEW box's interval, always.
- `introduce(words, date)`: unknown words enter at box 1, **due the same day** (a freshly completed stage is reviewable immediately); already-known words are never reset.
- `due_on(date)`: overdue plus due-today, alphabetical word order (deterministic sessions).
- Dates arrive as opaque strings; `add_days`/`valid_date` (Hinnant civil-date math, owned, no time crate) live here for the same reason `prev_day` lives in `progress/`.

Review grading reuses the engine: `session/` builds an `Exercise::Translation` per due card, passing the vocabulary entry's `accepted_translations` as the exercise's `accepted_answers` (D8), so reviews share the exact normalization table (accent hint included) with stage exercises. A card whose word vanished from the loaded content is skipped untouched (see §9).

## 5. `stages.json` content format

One file per language: `<content dir>/<lang>/stages.json` (see §7 for where content dirs are).

```json
{
  "language": "french",
  "stages": [
    {
      "id": 1,
      "title": "Greetings and Basics",
      "level_hint": "beginner",
      "vocabulary": [
        { "word": "bonjour", "translation": "hello", "pos": "interjection", "example": "Bonjour, comment ça va ?" }
      ],
      "exercises": [
        { "type": "translation", "prompt": "Je suis étudiant.", "answer": "I am a student", "accepted_answers": ["I'm a student"] },
        { "type": "multiple_choice", "prompt": "Choose the correct greeting:", "options": ["bonjour", "maison", "manger"], "answer": "bonjour" },
        { "type": "fill_blank", "prompt": "___, comment ça va ?", "answer": "Bonjour" }
      ]
    }
  ]
}
```

Rules:
- **No Arabic (or any non-English) text anywhere in this file.** Prompts, translations, examples: English. Only the target-language word/sentence itself (e.g. the French) is in the target language.
- `vocabulary` and `exercises` are **separate arrays**, never merged per-word.
- `level_hint` exists now (always `"beginner"` today) to avoid restructuring the file when higher levels are added later.
- `type` is the discriminator, exactly 3 values: `translation`, `multiple_choice`, `fill_blank`, handled by `#[serde(tag = "type", rename_all = "snake_case")]` in `content/mod.rs`.
- `accepted_answers` is optional on `translation` and `fill_blank`; absent = only `answer` counts. `accepted_translations` is the same field on vocabulary entries (D8): optional, absent = only `translation` counts; entries must be non-empty.
- All stages for a language currently live in one JSON file.

**Content validation** (settled in the review-fix session): `content::load_language` validates every file at load time and reports **all** violations at once. A file must have: at least one stage; stage ids unique and strictly ascending in file order; ≥ 1 exercise per stage (an empty stage can never be completed and would block the learner forever); multiple choice with 2–26 unique options and the answer among them; non-empty `prompt`/`answer`/`word`/`translation`; a `language` field that matches the name the file is loaded as; fill-blank prompts containing `___`; non-empty `accepted_answers` entries. `add-lang` prints the full violation list and installs nothing; `termocab check <file>` runs the same validator on any file without installing it.

## 6. Tech stack

- **Rust**, edition 2021. `rust-version = "1.78"` (the lockfile is v4, which older cargo cannot parse).
- **serde** + **serde_json** for content loading — the only dependencies. Deliberately **no** CLI crate (hand-rolled arg matching in `cli/`), **no** time crate (civil-date math in `progress/`/`srs/`, an owned TZif reader in `cli/time.rs` for the local timezone), **no** signal handling yet. Before adding any new dependency, explain why it's needed and whether it could be done without it.

## 7. Settled decisions — don't re-litigate these

- Rust, full project, no mixed C.
- Strict `engine/`/`progress/`/`srs/` ↔ storage separation (§3).
- `serde`/`serde_json` for content loading.
- JSON for language content (one file per language); plain text — not SQLite — for user state, both files: progress at `<config root>/progress/<lang>.txt`, srs at `<config root>/srs/<lang>.txt`, where the config root is `$XDG_CONFIG_HOME/termocab` (fallback `~/.config/termocab`), resolved at the CLI edge; with neither set this is an error, never the current directory. Strict parse: progress accepts only `stage <id>` and `day <real YYYY-MM-DD>` lines; srs accepts only `word <w> box <1-6> due <real YYYY-MM-DD>` lines (words may contain spaces — split on the ` box ` / ` due ` markers, not whitespace); anything else is an error. Saves go through temp-file + rename.
- **Content location** (review D2): looked up in order — `$TERMOCAB_CONTENT_DIR` (a directory containing `<lang>/stages.json`), then `$XDG_DATA_HOME/termocab/languages` (fallback `~/.local/share/termocab/languages`), then `./languages` for development. First directory that holds the language wins. `add-lang` always installs into the XDG data location. A missing language lists the directories searched.
- SRS is **classic Leitner, six boxes, intervals 1/2/4/8/16/32 days** (decided Oct 2026; SM-2 was considered and rejected for the MVP — more state, more complexity, no clear win at this scale). Correct promotes, wrong demotes to box 1, next due = today + interval(new box).
- SRS scheduling source: **vocabulary of completed stages only**, introduced lazily at box 1 due the same day. No forward scheduling of unseen material.
- **A review session with at least one answered card records an active day** — the streak stays alive through reviews, not just new stages.
- No `ncurses`/TUI in MVP — plain CLI text output only.
- Exactly 3 exercise types for MVP: translation, multiple_choice, fill_blank.
- Stage completion rule: **≥ 80% of exercises correct in one run**. A stage with 6 exercises passes at 5 correct; 4/6 fails.
- **Stages unlock in order** (review D4, the original spec's rule, now enforced): `start <lang> <n>` is allowed only when stage n is already completed (replay) or is the next uncompleted stage; anything else exits 1 with "complete stage X first".
- Saving is change-driven: a failed or aborted run writes nothing, and a passed replay on an already-recorded day rewrites nothing. Reviews always save the srs state when a card was answered, but rewrite the progress file only when the day is new.
- No placement system: starting a language always starts at Stage 1. `intermediate`/`advanced` are not CLI surface yet.
- No CEFR exposed to the user; sequential Stage number is the only level concept.
- Each Stage is sized for 10–15 minutes max.
- French is the first language; the engine stays language-agnostic.
- Grading (review D1): the normalization + `accepted_answers` rules of §4, with the accent-only mismatch counted correct plus a hint. Multiple choice accepts a letter (`b`) or the word itself.
- **Local time** (review D3): read from the system's own TZif file by `cli/time.rs` — no time crate, no libc; `$TZ` (name, absolute path, or empty = UTC) and `/etc/localtime` are honored, DST included, UTC on any parse failure. The old `TZHOURS` override is gone. Handlers take a `Clock`; tests inject fixed dates, never the wall clock.
- License: **MIT** (review D5).
- **Broken pipe** (usage-review D6): a closed stdout ends the run **silently with exit code 141** (128 + SIGPIPE), exactly what cat and git report — never a panic. Every stdout write is checked (`cli::say`, `Terminal::print -> io::Result`), an aborted session saves nothing, and EPIPE is distinguished from real write errors (which stay runtime failures, exit 1). Re-raising the actual signal would need unsafe libc calls, which §6 rules out; 141 is what the shell reports either way.
- **Shuffled stage exercises** (usage-review D7): stages present their exercises in a seeded-shuffled order by default — memorizing positions is not vocabulary. `--no-shuffle` keeps the authored order; `--seed <n>` pins the order (reproducible runs, tests, comparisons). The permutation is a hand-written Fisher–Yates over SplitMix64 (no `rand`, §6); the seed defaults to OS entropy (nanos ^ pid) resolved at the CLI edge, like the clock. Reviews are deliberately **not** shuffled: due-date order is the documented deterministic order.
- **Alternative translations in review** (usage-review D8): vocabulary entries take an optional `accepted_translations` array, the exact pattern of `accepted_answers` on exercises — a review of `salut` accepts both "hi" and "hello", through the same forgiving grading (case, accents, punctuation) as the primary translation. `list-words` shows the alternatives as `hi (also: hello)`.
- **Color** (usage-review): session feedback is green (correct) / red (not quite) **only when stdout is a terminal** and `$NO_COLOR` is unset or empty (no-color.org, to the letter — empty does not count as set). Piped output never contains escape codes. Decided at the CLI edge (`std::io::IsTerminal`, no dependency), passed into the session as a value.
- CLI is git-style direct subcommands only, named **start** (with `--no-shuffle`/`--seed <n>`, D7), **review**, **list-words**, **progress**, **streak**, **stats** (usage-review: boxes, 14-day review load and activity as plain-text Unicode sparklines, no color), **add-lang** (with `--force` to replace an existing language), **check**, **help**. No interactive/numbered-menu mode anywhere. Exit codes: 2 usage, 1 runtime, 0 success, 141 broken pipe (D6).

## 8. MVP scope — now implemented

**Done:** CLI, language selection (per-language content dirs + `add-lang`), Stage system, real French content (5 stages), vocabulary listing, three exercise types, stage progress tracking, plain-text persistent storage, stage completion, streaks.

## 8a. SRS layer — also implemented (Oct 2026)

**Done:** Leitner scheduling domain (`srs/`), plain-text persistence (`storage/`), review sessions behind the same `Terminal` abstraction (`session/`), `review` verb + due-count line in `progress` (`cli/`). Scheduling source is completed stages' vocabulary; reviews keep the streak alive (§7).

**Explicitly out of scope for now** (don't build unless asked): audio, in-app AI, online accounts, achievements, SM-2/EF-style scheduling, ncurses TUI, SQLite.

Known rough edges (fine to leave, fine to fix): none currently — the broken-pipe panic that used to live here is fixed (D6).

## 9. Remaining open questions

- Whether `content/`'s wire types stay permanently separate from `engine/`'s domain types. Current answer: they stay separate — don't collapse without a reason tied to §3.
- First-run content bootstrap: `cargo install` + `add-lang` works, but a fresh install has no content until the user adds some. Embedding the bundled French file with `include_str!` and installing it on first run was suggested and deliberately not done — decide when it matters.
- Orphan srs cards: a card whose word disappears from the content is skipped at review time and left in place forever. Fine at this scale; the honest fix is either pruning orphans on load or a `termocab` maintenance verb, to be decided when real content churn exists.
- Review session caps: a learner returning after weeks gets every overdue card in one sitting. Capping session size (e.g. newest-due-first, N per session) is a real UX question once there is enough content to matter.
- `cli/time.rs` covers `$TZ` names, absolute paths, empty-TZ and `/etc/localtime`; a POSIX TZ *string* (e.g. `EST5EDT,M3.2.0`) that has no zoneinfo file falls back to UTC. Fine until someone reports it.

## 10. Suggested next steps

**Content is the product**: the engine, validator and installer are ready for far more than five stages — the usage review's standing priorities are (1) a second language to prove the engine is language-agnostic, and (2) growing French, since every new stage completion feeds the srs automatically. `check` + `add-lang` already make external authoring safe; a community "content packs" flow (a repo of stages.json files) is the natural evolution of those two verbs — decide the shape when a second author shows up.

1. A second language to prove the engine is language-agnostic (`check` + `add-lang` make authoring easy).
2. More French stages (each new completion feeds the srs automatically).
3. ncurses TUI as an additive alternative frontend over `session::Terminal`.
4. SQLite storage layer replacing `storage/` (only that module changes).
5. Review session caps / ordering once content is large enough to need it (see §9).

## 11. Code quality bar

Every change should hold up against: Separation of Concerns, Single Responsibility, Low Coupling, High Cohesion, Abstraction, Encapsulation, Modularity, Error Handling, Testability, Maintainability. `cargo build` must stay warning-free and `cargo test` green — at the tip. CI (`.github/workflows/ci.yml`) enforces `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and `cargo build --release` on ubuntu-latest and macos-latest, on every push and PR — it is the review gate. The project owner reviews and tests changes personally — keep code clear and readable, not just functionally correct. Work in small, focused commits, since review happens through git history. The established pattern is one commit per layer (domain → storage → session → cli → docs, see the MVP, srs and review-fix waves in the log): an intermediate layer commit naturally carries dead-code warnings until the next commit wires it into the CLI, and `cargo test` stays green throughout — that is accepted; what is never accepted is a warning at the tip.

## 12. Build & run

```
cargo build          # warning-free
cargo test           # 93 unit + 22 binary-level integration tests
cargo run            # prints usage
cargo run -- start french       # play the next stage (shuffled by default, D7)
cargo run -- start --no-shuffle french 1   # authored exercise order
cargo run -- start --seed 42 french 1      # reproducible order
cargo run -- review french      # spaced-repetition review of due words
cargo run -- progress french    # per-stage checkboxes + streak + due count
cargo run -- stats french       # boxes + 14-day charts as sparklines
cargo run -- list-words french
cargo run -- streak french
cargo run -- add-lang spanish ./spanish.json     # installs into the XDG data dir
cargo run -- add-lang --force spanish ./new.json # replaces an installed language
cargo run -- check ./spanish.json                # validate without installing
```
