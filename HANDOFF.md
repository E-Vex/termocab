# HANDOFF — termocab

> This document is written for an AI coding agent taking over full development of **termocab**. It consolidates every decision made during the planning/bootstrap phase (Aug 2026), the MVP implementation session (Oct 2026), the review-fix session (Oct 2026), the usage-review session (Oct 2026), the second-language session (Nov 2026), the third-language session (Nov 2026), the fourth-language session (Nov 2026), the fifth-language session (Nov 2026) and the sixth-language session (Nov 2026) so no prior conversation context is needed to continue the project. **The repository itself is the source of truth** — if anything here conflicts with what you find in the actual code, trust the code, and update this file to match.

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

The **second-language session (Nov 2026)** shipped Spanish as the second course — the usage review's standing top priority — and proved the engine language-agnostic rather than French-agnostic: the accent table now covers the whole Latin-1 Supplement set (not 16 French letters), Spanish's inverted `¡` `¿` are edge punctuation, and `ß` opens like a ligature (D9). Both shipped courses are pinned by a binary-level `check` test in CI.

The **third-language session (Nov 2026)** shipped German as the third course with **zero engine changes** — the D9 table already covered it, which was the point of choosing it: `groß` and `heiß` make it the first shipped course to exercise `ß` in real content, and umlauts fold like any other accent ("tschuss" counts with `tschüss` as the hint). German is also the first course whose nouns carry their article (`der Mann`, `die Frau`, `das Mädchen`): stage 2's fill-the-blank exercises blank the article itself and stage 4's blank the infinitive after a modal (`Ich möchte einen Hund ___`) — the German-specific drills — while the srs keys and reviews keep the full accented, article-carrying word verbatim. All three shipped courses are pinned by the same binary-level `check` test in CI.

The **fourth-language session (Nov 2026)** shipped Italian as the fourth course with **zero engine changes** — the last pure-content candidate of the big Western European languages, so that set is now closed: every language the D9 table covers has a course, and the next language anyone adds will force the Latin Extended-A extension (§4). The table covered both of Italian's accent directions untouched — the grave (`sì`, `caffè`, `città`, `più`) and the acute (`perché`, `così`) — so "perche" and "caffe" grade correct with the properly spelled form as the hint. Italian takes German's articles-with-nouns convention and adds the elision (`l'uomo`, `l'amico` — either apostrophe grades, the srs key keeps the authored one verbatim): stage 2 blanks the article itself and shows one elision with the noun blanked (`L'___ legge il giornale`), stage 4 blanks the infinitive after a modal (`Voglio ___ un caffè`), and `perché` lists both directions of the why/because duality as accepted translations. All four shipped courses are pinned by the same binary-level `check` test in CI.

The **fifth-language session (Nov 2026)** shipped Polish as the fifth course and, with it, the **first engine change a language has forced since D9 (D10)** — the live test of the extension path the accent table was designed for. The seven decomposable Polish letters (ą ć ę ń ś ź ż — ogonek, acute, dot above) joined the `ACCENTED` table as base + combining mark, riding the existing strict/loose machinery unchanged; ł, which has no Unicode decomposition, opens in the loose pass to plain `l` exactly like `ß` opens to `ss` — same word, missing stroke, properly spelled form as the hint, in a new `OPEN_LETTERS` table beside the ligatures. The change is purely additive: none of the four shipped courses contains a new letter, so nothing shifted under them. Polish has no articles, so its nouns are bare and the stage-2 drill is the identification pattern instead ("___ jest mój brat" wants "to" — the German-article slot, Polish edition); stage 4 keeps the family drill, chcieć/lubię/muszę + blanked infinitive ("Chcę ___ kawę" wants "pić"); and all nine Polish letters are alive in real content (piątek, pięć, dzień, krótki, szczęśliwy, jeździć, książki). All five shipped courses are pinned by the same binary-level `check` test in CI.

The **sixth-language session (Nov 2026)** shipped Romanian as the sixth course and the **second engine change a language has forced (D11)** — and half of it upended the D10 prediction: ș and ț, which were expected to have no decomposition and to follow ł into `OPEN_LETTERS`, decompose after all (s/t + combining comma below), so the whole Romanian set joined `ACCENTED` beside the Polish letters, with the legacy cedilla spellings ş ţ riding along so both historical encodings grade the same way. `OPEN_LETTERS` still holds just ł; the change is purely additive — no other shipped course contains a new letter, so nothing shifted under the other five. Romanian bends both imported conventions to the language: its definite article is enclitic, so stage 2 blanks it at the END of the noun (`Băiat___ citește ziarul.` wants `ul` — the German-article slot, Romanian edition), and Romanian has no want + infinitive, so stage 4's modal drill is pot/poți/putem + the bare infinitive (`Pot ___ românește.` wants `vorbi`), with the a-prefixed dictionary infinitive accepted as an alternative and the vocabulary listing infinitives the Romanian way (`a citi`). All five letters ă â î ș ț are alive in real content (bună ziua, mâine, înțeleg, șapte, mulțumesc). All six shipped courses are pinned by the same binary-level `check` test in CI.

```
termocab/
├── Cargo.toml              # deps: serde (derive), serde_json — nothing else
├── Cargo.lock
├── .github/workflows/ci.yml  # fmt + clippy -D warnings + test + release build, ubuntu & macos
├── .gitignore
├── LICENSE                 # MIT
├── README.md
├── languages/
│   ├── french/
│   │   └── stages.json     # first course: 5 stages, 73 words, 177 exercises
│   ├── spanish/
│   │   └── stages.json     # second course (D9): 5 stages, 73 words, 197 exercises
│   ├── german/
│   │   └── stages.json     # third course: 5 stages, 73 words, 216 exercises
│   │                       #   (articles on nouns, modals + infinitive blanks)
│   ├── italian/
│   │   └── stages.json     # fourth course: 5 stages, 73 words, 218 exercises
│   │                       #   (articles + elisions on nouns, modals +
│   │                       #    infinitive blanks, grave and acute accents)
│   ├── polish/
│   │   └── stages.json     # fifth course (D10): 5 stages, 73 words, 218 exercises
│   │                       #   (bare nouns — no articles in Polish, "to jest"
│   │                       #    blanks, chcieć/lubię + infinitive blanks, all
│   │                       #    nine Polish letters in real content)
│   └── romanian/
│       └── stages.json     # sixth course (D11): 5 stages, 73 words, 218 exercises
│                           #   (enclitic definite article blanks — the article
│                           #    comes AFTER the noun — pot + infinitive
│                           #    blanks, all five Romanian letters in real
│                           #    content)
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

Verified: `cargo build` warning-free, `cargo clippy --all-targets -- -D warnings` clean, `cargo test` green (96 unit + 28 binary-level integration tests), a scripted playthrough of the full French course passes end to end (5/5 stages, streak, replay, 73-card review), a scripted playthrough of the full Spanish course does the same with deliberately accent-stripped answers (5/5 stages, 73/73 review, every card promoted to box 2), a scripted playthrough of the full German course passes with umlaut- and sharp-s-stripped answers (5/5 stages, 73/73 review, every card promoted to box 2, `Brüder` hinted after `Bruder`), a scripted playthrough of the full Italian course passes with accent-stripped answers (5/5 stages, 73/73 review, every card promoted to box 2, `sì` hinted after `si`), a scripted playthrough of the full Polish course passes with diacritic- and stroke-stripped answers (5/5 stages, 63 accent hints, 73/73 review, every card promoted to box 2, `dziękuję` hinted after `dziekuje`, `żółty` after `zolty`), and a scripted playthrough of the full Romanian course passes with diacritic-stripped answers (5/5 stages, 13 accent hints, 73/73 review, every card promoted to box 2, `mulțumesc` hinted after `multumesc`, `frați` after `frati`). CI (§11) runs all of it on ubuntu and macOS.

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

Grading (settled in the review-fix session, decision D1; generalized by D9): both sides go through `normalize_strict` — trim, lowercase, collapse internal whitespace, strip leading/trailing punctuation (`. ? ! , ;` and Spanish's inverted `¡ ¿`), fold apostrophes (`’` → `'`), and fold Unicode composition via a hand-written 39-entry accent table — the Latin-1 Supplement set (à á â ã ä å ç é è ê ë ì í î ï ñ ò ó ô õ ö ù ú û ü ý ÿ), the decomposable Polish letters of Latin Extended-A, ą ć ę ń ś ź ż (D10), and the Romanian set, ă ș ț ş ţ (D11) — so precomposed `é` equals macOS's `e` + U+0301, and precomposed `ń` equals hand-typed `n` + U+0301: same letter, same accent, different encoding. A second, loose pass drops the combining marks and opens the œ/æ/ß ligatures and the stroked ł (D10): an accent-only or stroke-only mismatch is **correct**, and `grade` returns `CorrectWithHint` carrying the properly spelled form, which the session prints as `correct — watch the accents: étudiant` (or `łódź`). Letters that never fold (`ø`, `þ`, `ð`) deliberately stay closed — `smør` is not `smor`; ł does fold, because it is the same word missing a stroke — the ß rule. `accepted_answers` (optional field on translation and fill_blank, absent in old files) lists alternative correct spellings — contractions ("I'm a student"), gendered forms ("Je suis étudiante").

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
- **Language-agnostic normalization** (second-language session, D9): the accent table is the accented-letter set of the whole Latin-1 Supplement — 27 lowercase entries covering Western Europe (Spanish, German, Italian, Portuguese, Catalan, Icelandic…), not a French dialect; `¡` and `¿` join the edge-punctuation set (`hola` is `¡Hola!`); `ß` opens to `ss` in the loose pass exactly like `œ` → `oe`. Letters that are letters in their own right (`ø`, `þ`, `ð`) never fold. When a language outside the set arrives (Polish, Romanian, …), extend the table with Latin Extended-A entries — same pattern, still no Unicode-normalization dependency.
- **Latin Extended-A, the Polish set** (fifth-language session, D10): the D9 prediction, executed. The seven decomposable Polish letters (ą ć ę ń ś ź ż) joined `ACCENTED` as base + combining mark — ogonek U+0328, acute U+0301, dot above U+0307, all inside the loose pass's existing combining range, so the loose pass needed no change at all. ł, which has no Unicode decomposition, joined `ß`'s loose-only opening rule as a new `OPEN_LETTERS` table beside the ligatures: `łódź` counts with `lodz` as the hint, exactly like `straße` with `strasse`. ø þ ð stay closed (D9 stands). This decision also predicted how Romanian would land — ă via `ACCENTED`, ș and ț via `OPEN_LETTERS` — and the prediction was half wrong; see D11 for the corrected outcome. Still no Unicode-normalization dependency.
- **Latin Extended-A/B, the Romanian set** (sixth-language session, D11): the second live test of the extension path, and the outcome was smaller than predicted. ă joined `ACCENTED` as expected (a + breve), but ș and ț turned out to decompose too (s/t + combining comma below, U+0326 — Extended-B included), so they joined `ACCENTED` beside the Polish letters instead of following ł into `OPEN_LETTERS`; the legacy cedilla spellings ş ţ (s/t + cedilla, the mark ç already carries) joined as well, so both historical encodings of the same two letters grade the same way — `şarpe` counts with `șarpe` as the hint. `OPEN_LETTERS` still holds just ł. The change is purely additive: no other shipped course contains a new letter, so nothing shifted under the other five. Still no Unicode-normalization dependency.
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

**Content is the product**: six courses now ship (French, Spanish, German, Italian, Polish, Romanian — 73 words each, the same five-stage skeleton), and the engine has needed exactly two table extensions to absorb all of them (D9, D10, D11). The prediction track record is now one for two: D9 correctly predicted the Latin Extended-A extension for Polish, and D10's prediction for Romanian was half right — ă joined `ACCENTED` as expected, but ș and ț decompose after all, so `OPEN_LETTERS` gained nothing. `check` + `add-lang` make outside authoring safe, and a community "content packs" flow (a repo of stages.json files) is the natural evolution of those two verbs. The standing priorities: (1) growing the six shipped courses, since every new stage completion feeds the srs automatically, and (2) a seventh language — Portuguese is the natural candidate: it is already inside the D9 table (ã õ ç â ê ô and the acute vowels), so it should ship with zero engine changes — the third live test of the extension path, this time proving the no-change path German and Italian took.

1. More French, Spanish, German, Italian, Polish and Romanian stages (each new completion feeds the srs automatically).
2. A seventh language: Portuguese — pure content, zero engine changes expected; the same live test German and Italian passed.
3. ncurses TUI as an additive alternative frontend over `session::Terminal`.
4. SQLite storage layer replacing `storage/` (only that module changes).
5. Review session caps / ordering once content is large enough to need it (see §9).
6. Content packs — a repo of community stages.json files validated by `check`.

## 11. Code quality bar

Every change should hold up against: Separation of Concerns, Single Responsibility, Low Coupling, High Cohesion, Abstraction, Encapsulation, Modularity, Error Handling, Testability, Maintainability. `cargo build` must stay warning-free and `cargo test` green — at the tip. CI (`.github/workflows/ci.yml`) enforces `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and `cargo build --release` on ubuntu-latest and macos-latest, on every push and PR — it is the review gate. The project owner reviews and tests changes personally — keep code clear and readable, not just functionally correct. Work in small, focused commits, since review happens through git history. The established pattern is one commit per layer (domain → storage → session → cli → docs, see the MVP, srs and review-fix waves in the log): an intermediate layer commit naturally carries dead-code warnings until the next commit wires it into the CLI, and `cargo test` stays green throughout — that is accepted; what is never accepted is a warning at the tip.

## 12. Build & run

```
cargo build          # warning-free
cargo test           # 96 unit + 28 binary-level integration tests
cargo run            # prints usage
cargo run -- start french       # play the next stage (shuffled by default, D7)
cargo run -- start spanish      # the second course, same engine (D9)
cargo run -- start german       # the third course: articles, umlauts, ß
cargo run -- start italian      # the fourth course: elisions, grave + acute
cargo run -- start polish       # the fifth course: the D10 wave, ogoneks + the stroke
cargo run -- start romanian     # the sixth course: the D11 wave, enclitic articles, ă â î ș ț
cargo run -- start --no-shuffle french 1   # authored exercise order
cargo run -- start --seed 42 french 1      # reproducible order
cargo run -- review italian     # spaced-repetition review of due words
cargo run -- progress italian   # per-stage checkboxes + streak + due count
cargo run -- stats italian      # boxes + 14-day charts as sparklines
cargo run -- list-words italian
cargo run -- streak italian
cargo run -- add-lang italian ./italian.json     # installs into the XDG data dir
cargo run -- add-lang --force italian ./new.json # replaces an installed language
cargo run -- check ./italian.json                # validate without installing
cargo run -- check languages/italian/stages.json # validate a shipped course
```
