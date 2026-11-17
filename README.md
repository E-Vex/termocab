<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="termocab" width="420">
  </picture>
</p>

<p align="center">
  <strong>Language learning for terminal enthusiasts.</strong><br>
  Staged lessons, spaced-repetition reviews and streaks —<br>
  fully offline, no account, and your progress is a text file you can <code>cat</code>.
</p>

<p align="center">
  <a href="https://github.com/E-Vex/termocab/actions/workflows/ci.yml"><img src="https://github.com/E-Vex/termocab/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/rust-1.78%2B-orange.svg" alt="Rust 1.78+">
  <img src="https://img.shields.io/badge/platform-linux%20%7C%20macOS-lightgrey.svg" alt="Linux and macOS">
  <img src="https://img.shields.io/badge/courses-13-2dd4bf.svg" alt="13 courses">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#courses">Courses</a> ·
  <a href="#forgiving-grading">Grading</a> ·
  <a href="#bring-your-own-language">Your own language</a> ·
  <a href="#development">Development</a>
</p>

---

## Why termocab

- **Terminal-native.** Git-style subcommands: `termocab start french`, `termocab review french`. No menus, no TUI, no mouse.
- **Offline and private.** All 13 courses are compiled into a single binary. No account, no network, nothing to sync.
- **Plain text everywhere.** Courses are JSON. Progress and review state are small text files you can read, diff and back up.
- **Real spaced repetition.** Leitner boxes (1 / 2 / 4 / 8 / 16 / 32 days) bring words back right before you forget them.
- **Forgiving where it should be.** Case, spacing and missing accents never cost you a point: `perche` is accepted for `perché`, and shows you the right spelling.
- **Small.** Rust, standard library plus `serde`. No CLI framework, no time library.

## Quick start

```sh
cargo install --git https://github.com/E-Vex/termocab
termocab start french        # play the next stage
termocab review french       # later: revisit the words that are due
```

A stage looks like this, abridged (you type the line under each prompt):

```text
$ termocab start french
— Stage 1: Greetings and Basics —
32 exercise(s). Answer each, then press Enter.

[Greetings and Basics — 1/32] bonjour
hello
correct.
…
[Greetings and Basics — 12/32] see you soon
a bientot
correct — watch the accents: à bientôt
…
[Greetings and Basics — 14/32] Choose the word for 'thank you':
  a. merci
  b. bonjour
  c. oui
  d. salut
(answer with a letter or the word itself)
a
correct.
…
[Greetings and Basics — 23/32] ___, comment ça va ?
Bonjour
correct.
…
stage complete: 28/32 correct. nice.
progress saved.
```

Your numbers, one command away:

```text
$ termocab stats french
french — 1/15 stage(s) complete, streak 1 day(s)
words: 233 in content, 12 scheduled, 12 due today
boxes 1-6: █▁▁▁▁▁  (12 0 0 0 0 0)
reviews due, next 14 days (today first): █▁▁▁▁▁▁▁▁▁▁▁▁▁
active days, last 14 (oldest first): ▁▁▁▁▁▁▁▁▁▁▁▁▁█
```

### Install

| Method | Command |
| --- | --- |
| From GitHub | `cargo install --git https://github.com/E-Vex/termocab` |
| From a checkout | `cargo install --path .` |
| Without installing | `cargo run --release -- start french` |
| From crates.io | `cargo install termocab` — goes live with the first tagged release |

Needs Rust 1.78 or newer, on Linux or macOS. If your shell says `termocab: command not found`, add `~/.cargo/bin` to your `$PATH`.

## Commands

| Command | What it does |
| --- | --- |
| `termocab start [--no-shuffle \| --seed <n>] <lang> [stage]` | Run the next uncompleted stage, or a specific one. Exercises shuffle by default; `--seed` makes the order reproducible. |
| `termocab review <lang>` | Review the words that are due today. |
| `termocab progress <lang>` | Completed stages, streak and due reviews. |
| `termocab stats <lang>` | Leitner boxes and 14-day charts as sparklines. |
| `termocab streak <lang>` | Print the current streak in days. |
| `termocab list-words <lang>` | Every word in the course, with examples. |
| `termocab list-langs` | Available languages, and where each one comes from. |
| `termocab add-lang [--force] <lang> <file>` | Install a `stages.json` file as a language. |
| `termocab check <file>` | Validate a `stages.json` file without installing it. |
| `termocab --version` · `termocab help` | Print the version / show usage. |

**The rules.** Stages unlock in order, and a finished stage can be replayed any time. A stage is passed with **80% or more correct** in one run. Reviews follow the Leitner boxes above.

**Good citizen.** Colour (green / red feedback) only appears on a terminal, pipes stay plain, and `$NO_COLOR` turns it off everywhere. Exit codes follow git: `0` success, `1` runtime failure, `2` usage error. A closed pipe (`termocab list-words french | head`) ends quietly with `141`, like `cat` does.

## Courses

Thirteen courses ship inside the binary. Every course has 15 stages in three waves, **beginner** (1–5), **elementary** (6–10) and **intermediate** (11–15), and runs on the same engine with the same rules.

| Course | Stages | Words | Exercises |
| --- | :---: | :---: | :---: |
| French | 15 | 233 | 617 |
| Spanish | 15 | 233 | 637 |
| German | 15 | 233 | 656 |
| Italian | 15 | 233 | 658 |
| Polish | 15 | 233 | 658 |
| Romanian | 15 | 233 | 658 |
| Portuguese (Brazilian) | 15 | 233 | 658 |
| Swedish | 15 | 233 | 658 |
| Hungarian | 15 | 233 | 658 |
| Turkish | 15 | 233 | 658 |
| Icelandic | 15 | 233 | 658 |
| Catalan | 15 | 233 | 658 |
| Dutch | 15 | 233 | 658 |

Each course keeps its own signature drills: articles blanked where they live in Romanian, Swedish and Icelandic, vowel harmony in Hungarian and Turkish, the separable verbs of Dutch, and more. See [docs/courses.md](docs/courses.md).

## Forgiving grading

Grading forgives case, whitespace, edge punctuation and missing accents, and an accent-only miss counts as correct and shows you the accented form. Letters that are a different letter, not an accented one, stay strict.

| You type | Expected | Result |
| --- | --- | --- |
| `perche` | `perché` | ✅ correct, accent shown |
| `strasse` | `Straße` | ✅ correct, `ß` shown |
| `lodz` | `łódź` | ✅ correct, accents and stroke shown |
| `het IJsselmeer` | `het Ĳsselmeer` | ✅ correct, ligature shown |
| `hayir` | `hayır` | ❌ miss: `ı` is a different vowel |
| `godur` | `góður` | ❌ miss: `ð` stays closed |

The full rules, per language, live in [docs/grading.md](docs/grading.md).

## Where things live

Course content is looked up in this order, first hit wins:

1. `$TERMOCAB_CONTENT_DIR`, a directory containing `<lang>/stages.json`
2. `$XDG_DATA_HOME/termocab/languages` (or `~/.local/share/termocab/languages`), where `add-lang` installs. A language installed here replaces a built-in one of the same name.
3. `./languages`, so `cargo run` works from a checkout
4. the courses built into the binary

Progress and review state sit under `$XDG_CONFIG_HOME/termocab` (or `~/.config/termocab`), one text file per language in each of `progress/` and `srs/`:

```text
$ cat ~/.config/termocab/srs/french.txt
# termocab srs
# word <word> box <1-6> due <YYYY-MM-DD>
word bonjour box 1 due 2026-11-18
word merci box 1 due 2026-11-17
```

`termocab list-langs` shows every available language and where it comes from.

## Bring your own language

A course is one `stages.json` file:

```json
{
  "language": "french",
  "stages": [
    {
      "id": 1, "title": "Greetings and Basics", "level_hint": "beginner",
      "vocabulary": [
        { "word": "bonjour", "translation": "hello",
          "accepted_translations": ["hi", "good morning"],
          "pos": "interjection", "example": "Bonjour, comment ça va ?" }
      ],
      "exercises": [
        { "type": "translation", "prompt": "bonjour", "answer": "hello" },
        { "type": "multiple_choice", "prompt": "Choose the word for 'hello':",
          "options": ["bonjour", "au revoir", "merci", "non"], "answer": "bonjour" },
        { "type": "fill_blank", "prompt": "___, comment ça va ?", "answer": "Bonjour" }
      ]
    }
  ]
}
```

```sh
termocab check my-language.json              # validate; prints every problem at once
termocab add-lang my-language my-language.json
```

`accepted_answers` (exercises) and `accepted_translations` (vocabulary) list alternative correct answers. The complete format and its rules are in [HANDOFF.md](HANDOFF.md) §5.

## Development

```sh
cargo build                                   # warning-free
cargo test                                    # unit + binary-level integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI runs all of the above on Ubuntu and macOS.

| Document | What is in it |
| --- | --- |
| [HANDOFF.md](HANDOFF.md) | Architecture, domain model and every settled decision |
| [RELEASE.md](RELEASE.md) | How releases are cut, and what semver means for course content |
| [CHANGELOG.md](CHANGELOG.md) | Release history |
| [docs/](docs/) | Per-course details and the full grading rules |

**Releases** are cut by tag: pushing `vX.Y.Z` makes the Release workflow verify it (fmt, clippy, tests, release build, publish dry-run), publish to crates.io and attach Linux and macOS binaries. One version number covers the engine, the CLI and the courses. Additive content is a MINOR bump; anything that could strand a learner's saved state is a MAJOR bump.

## License

[MIT](LICENSE) © 2026 E-Vex
