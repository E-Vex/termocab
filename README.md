# termocab

**Language learning for terminal enthusiasts.**

A terminal-native vocabulary trainer: staged lessons, spaced-repetition
reviews, streaks — fully offline, fully local, no account, no network.
Plain text everywhere: content is JSON, your progress is a file you can
`cat`. UX modeled on git — direct subcommands, no menus.

The first course is French: five stages, 73 words, 177 exercises.

## Install & run

```
cargo install --path .
termocab start french
```

Content is looked up in this order (first hit wins):

1. `$TERMOCAB_CONTENT_DIR` — a directory containing `<lang>/stages.json`
2. `$XDG_DATA_HOME/termocab/languages`, or `~/.local/share/termocab/languages`
   (this is where `add-lang` installs)
3. `./languages` — development convenience, so `cargo run` works from a
   checkout

Progress and review state live under `$XDG_CONFIG_HOME/termocab`
(fallback `~/.config/termocab`), one plain-text file per language.

## Commands

```
termocab start <lang> [stage]     run the next uncompleted stage (or a specific one)
termocab review <lang>            review due vocabulary with spaced repetition
termocab list-words <lang>        list all vocabulary in a language's stages
termocab progress <lang>          show completed stages, streak and due reviews
termocab streak <lang>            print the current streak in days
termocab add-lang [--force] <lang> <file>  install a stages.json file as a language
termocab check <file>             validate a stages.json file without installing it
termocab help                     show this help
```

Exit codes follow git: 0 success, 1 runtime failure, 2 usage error.
Stages unlock in order (a completed stage can be replayed anytime).
A stage is passed at ≥ 80% correct in one run; reviews use Leitner boxes
with 1/2/4/8/16/32-day intervals.

## Content format

One `stages.json` per language — see `languages/french/stages.json` for a
full example and `HANDOFF.md` §5 for the rules. Quick sketch:

```json
{
  "language": "french",
  "stages": [
    {
      "id": 1, "title": "Greetings and Basics", "level_hint": "beginner",
      "vocabulary": [
        { "word": "bonjour", "translation": "hello",
          "pos": "interjection", "example": "Bonjour, comment ça va ?" }
      ],
      "exercises": [
        { "type": "translation", "prompt": "bonjour", "answer": "hello" },
        { "type": "translation", "prompt": "Je suis étudiant.", "answer": "I am a student.",
          "accepted_answers": ["I'm a student"] },
        { "type": "multiple_choice", "prompt": "Choose the word for 'hello':",
          "options": ["bonjour", "au revoir", "merci", "non"], "answer": "bonjour" },
        { "type": "fill_blank", "prompt": "___, comment ça va ?", "answer": "Bonjour" }
      ]
    }
  ]
}
```

Grading forgives case, whitespace, edge punctuation and missing accents
(an accent-only miss counts as correct and shows the accented form);
`accepted_answers` lists alternative correct spellings. Validate a file
before installing it:

```
termocab check my-language.json
termocab add-lang my-language my-language.json
```

## Status

Early and actively developed. Rust (no CLI framework, no time library —
stdlib plus serde only), Linux and macOS, MIT licensed. The architecture
and every settled decision are documented in `HANDOFF.md`.

## Development

```
cargo build            # warning-free
cargo test             # unit + binary-level integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI runs all of the above on ubuntu and macOS.
