# HANDOFF — termocab

> This document is written for an AI coding agent taking over full development of **termocab**. It consolidates every decision made during the planning/bootstrap phase (Aug 2026) and the MVP implementation session (Oct 2026) so no prior conversation context is needed to continue the project. **The repository itself is the source of truth** — if anything here conflicts with what you find in the actual code, trust the code, and update this file to match.

## 1. What this is

termocab is an open-source, terminal-native language-learning CLI. Tagline: **"Language learning for terminal enthusiasts."**

Non-negotiable philosophy:
- Runs entirely in the terminal. No GUI, no browser, no server, no network dependency. Fully offline, fully local.
- Fast, lightweight, keyboard-driven.
- No ads, no mandatory account, respects user data.
- UX modeled on real Unix tools — **git** is the explicit style reference: clear, direct subcommands. No interactive menus, no numbered prompts waiting for input. Every command is a complete, self-contained verb: `termocab <subcommand> [args]`.

Name history: `ling` → `vex-lang` → **`termocab`** (current, final). An earlier, broader `vex-lang` vision doc described a full TUI + detailed SRS system — that is a long-term reference only, **not** current MVP scope.

## 2. Current status

The MVP described in §8 is **implemented and verified**: engine grading, progress tracking, plain-text storage, session runner, and the full CLI all work end-to-end (French Stage 1 can be played to completion and the result persists).

The **SRS layer (§8a) is also implemented and verified** (Oct 2026): Leitner-box scheduling, plain-text persistence, a `review` verb, and a due-count line in `progress`. A learner who completes Stage 1 gets its 8 words scheduled immediately; reviews reschedule them and keep the streak alive.

```
termocab/
├── Cargo.toml              # deps: serde (derive), serde_json — nothing else
├── Cargo.lock
├── languages/
│   └── french/
│       └── stages.json     # sample fixture, 1 stage, 8 words, 6 exercises
└── src/
    ├── main.rs              # thin entry: collects args, calls cli::run
    ├── engine/
    │   └── mod.rs           # domain types + grading + 80% completion rule
    ├── content/
    │   └── mod.rs           # JSON wire types → engine types (unchanged)
    ├── progress/
    │   └── mod.rs           # LanguageProgress domain type, streak logic, tests
    ├── srs/
    │   └── mod.rs           # Leitner scheduling domain: boxes, due dates, tests
    ├── storage/
    │   └── mod.rs           # plain-text persistence for progress AND srs state
    ├── session/
    │   └── mod.rs           # stage runner + review runner behind a Terminal trait
    └── cli/
        └── mod.rs           # subcommand dispatch, date math, tests
```

Verified: `cargo build` is warning-free, `cargo test` passes 35 tests, and a full manual smoke test of every subcommand succeeded — including a two-day review flow (complete stage → review → nothing due → next-day review → streak grows). No dependencies beyond serde/serde_json.

## 3. Architecture — read this before writing any code

Strict layered separation:

```
Terminal/CLI  →  Application Logic  →  Learning Engine  →  Progress Tracker  →  Storage Layer
                                    ↘ SRS Scheduler ↗
```

**The single most important rule in this codebase:** `engine/`, `progress/` and `srs/` must never know how anything is stored. No JSON, no file I/O, no SQLite — ever. They only work with plain in-memory types. `content/` and `storage/` are the only modules allowed to know both about the domain types and about a storage format at the same time; they convert between the two. This is what lets the project move from JSON/plain-text to SQLite later by rewriting only the storage-facing modules.

`session/` is the application layer: it knows `engine::`, `progress::` and `srs::` but not files or dates. `cli/` is the only module that touches `std::env`, exit codes, and "what day is it". The `Terminal` trait in `session/` abstracts stdin/stdout so tests drive the runner with scripted answers.

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
    pub pos: String,
    pub example: String,
}

pub enum Exercise {
    Translation { prompt: String, answer: String },
    MultipleChoice { prompt: String, options: Vec<String>, answer: String },
    FillBlank { prompt: String, answer: String },
}
```

Engine API added Oct 2026: `Exercise::prompt()`, `Exercise::answer()`, `Exercise::is_correct(attempt)` (case- and whitespace-insensitive), and `Stage::is_completed_by(correct_count)` implementing the 80% rule via `STAGE_PASS_FRACTION`.

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

Review grading reuses the engine: `session/` builds an `Exercise::Translation` per due card, so reviews share the exact case-insensitive grading rule with stage exercises. A card whose word vanished from the loaded content is skipped untouched (see §9).

## 5. `stages.json` content format

One file per language: `languages/<lang>/stages.json`.

```json
{
  "language": "french",
  "stages": [
    {
      "id": 1,
      "title": "First Steps",
      "level_hint": "beginner",
      "vocabulary": [
        { "word": "bonjour", "translation": "hello", "pos": "interjection", "example": "Bonjour, comment ca va ?" }
      ],
      "exercises": [
        { "type": "translation", "prompt": "Je suis etudiant.", "answer": "I am a student" },
        { "type": "multiple_choice", "prompt": "Choose the correct greeting:", "options": ["bonjour", "maison", "manger"], "answer": "bonjour" },
        { "type": "fill_blank", "prompt": "___, comment ca va ?", "answer": "Bonjour" }
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
- All stages for a language currently live in one JSON file.

## 6. Tech stack

- **Rust**, edition 2021.
- **serde** + **serde_json** for content loading — the only dependencies. Deliberately **no** CLI crate (hand-rolled arg matching in `cli/`), **no** time crate (civil-date math in `cli/`, calendar math in `progress/`), **no** signal handling yet. Before adding any new dependency, explain why it's needed and whether it could be done without it.

## 7. Settled decisions — don't re-litigate these

- Rust, full project, no mixed C.
- Strict `engine/`/`progress/`/`srs/` ↔ storage separation (§3).
- `serde`/`serde_json` for content loading.
- JSON for language content (one file per language); plain text — not SQLite — for user state, both files: progress at `$XDG_CONFIG_HOME/termocab/progress/<lang>.txt`, srs at `…/termocab/srs/<lang>.txt` (fallback `~/.config/...`). Strict parse: progress accepts only `stage <id>` and `day <date>` lines; srs accepts only `word <w> box <1-6> due <YYYY-MM-DD>` lines (words may contain spaces — split on the ` box ` / ` due ` markers, not whitespace); anything else is an error. Saves go through temp-file + rename.
- SRS is **classic Leitner, six boxes, intervals 1/2/4/8/16/32 days** (decided Oct 2026; SM-2 was considered and rejected for the MVP — more state, more complexity, no clear win at this scale). Correct promotes, wrong demotes to box 1, next due = today + interval(new box).
- SRS scheduling source: **vocabulary of completed stages only**, introduced lazily at box 1 due the same day. No forward scheduling of unseen material.
- **A review session with at least one answered card records an active day** — the streak stays alive through reviews, not just new stages.
- No `ncurses`/TUI in MVP — plain CLI text output only.
- Exactly 3 exercise types for MVP: translation, multiple_choice, fill_blank.
- CLI is git-style direct subcommands only, named **start**, **review**, **list-words**, **progress**, **streak**, **add-lang**, **help** (review added Oct 2026 with the srs layer). No interactive/numbered-menu mode anywhere. Exit codes: 2 usage, 1 runtime, 0 success.
- Stage completion rule: **≥ 80% of exercises correct in one run** (decided Oct 2026, was open in §9). A stage with 6 exercises passes at 5 correct; 4/6 fails.
- No placement system: starting a language always starts at Stage 1. `intermediate`/`advanced` are not CLI surface yet (the old §9 open question is resolved by "don't offer it until higher-level content exists").
- No CEFR exposed to the user; sequential Stage number is the only level concept.
- Each Stage is sized for 10–15 minutes max.
- French is the first language; the engine stays language-agnostic.
- Grading is case-insensitive and trims whitespace; multiple choice accepts a letter (`b`) or the word itself.

## 8. MVP scope — now implemented

**Done:** CLI, language selection (per-language content dirs + `add-lang`), Stage system, real French sample content, vocabulary listing, three exercise types, stage progress tracking, plain-text persistent storage, stage completion, streaks.

## 8a. SRS layer — also implemented (Oct 2026)

**Done:** Leitner scheduling domain (`srs/`), plain-text persistence (`storage/`), review sessions behind the same `Terminal` abstraction (`session/`), `review` verb + due-count line in `progress` (`cli/`). Scheduling source is completed stages' vocabulary; reviews keep the streak alive (§7).

**Explicitly out of scope for now** (don't build unless asked): audio, in-app AI, online accounts, achievements, SM-2/EF-style scheduling, ncurses TUI, SQLite.

Known rough edges (fine to leave, fine to fix):
- Content dir is `languages/` relative to cwd (or `$TERMOCAB_CONTENT_DIR`); no install-time content location yet.
- `TZHOURS` env var shifts the streak date from UTC; std can't read the local timezone — a deliberate tradeoff documented in `cli/mod.rs`.
- Broken pipe on `list-words | head` panics (Rust default SIGPIPE handling); cosmetic, noted in the Oct 2026 session.

## 9. Remaining open questions

- Whether `content/`'s wire types stay permanently separate from `engine/`'s domain types. Current answer: they stay separate — don't collapse without a reason tied to §3.
- Real French content: the shipped `stages.json` is still the sample fixture. The owner may replace it with real stages.
- Multi-stage ordering guarantees when stage ids are non-sequential in a file (engine iterates file order today).
- Installing the binary via cargo + a first-run content bootstrap (`add-lang` requires a local file today).
- Orphan srs cards: a card whose word disappears from the content is skipped at review time and left in place forever. Fine at this scale; the honest fix is either pruning orphans on load or a `termocab` maintenance verb, to be decided when real content churn exists.
- Review session caps: a learner returning after weeks gets every overdue card in one sitting. Capping session size (e.g. newest-due-first, N per session) is a real UX question once there is enough content to matter.

## 10. Suggested next steps

1. Real content: more French stages (each new completion feeds the srs automatically), then a second language to prove the engine is language-agnostic.
2. ncurses TUI as an additive alternative frontend over `session::Terminal`.
3. SQLite storage layer replacing `storage/` (only that module changes).
4. Review session caps / ordering once content is large enough to need it (see §9).
5. CI (build + test on push) once the owner wants it.

## 11. Code quality bar

Every change should hold up against: Separation of Concerns, Single Responsibility, Low Coupling, High Cohesion, Abstraction, Encapsulation, Modularity, Error Handling, Testability, Maintainability. `cargo build` must stay warning-free and `cargo test` green — at the tip. The project owner reviews and tests changes personally — keep code clear and readable, not just functionally correct. Work in small, focused commits, since review happens through git history. The established pattern is one commit per layer (domain → storage → session → cli → docs, see both the MVP and srs waves in the log): an intermediate layer commit naturally carries dead-code warnings until the next commit wires it into the CLI, and `cargo test` stays green throughout — that is accepted; what is never accepted is a warning at the tip.

## 12. Build & run

```
cargo build          # warning-free
cargo test           # 35 tests
cargo run            # prints usage
cargo run -- start french       # play the next stage
cargo run -- review french      # spaced-repetition review of due words
cargo run -- progress french    # per-stage checkboxes + streak + due count
cargo run -- list-words french
cargo run -- streak french
cargo run -- add-lang spanish ./spanish.json
```
