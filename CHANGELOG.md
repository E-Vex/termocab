# Changelog

All notable changes to termocab are documented here. Versions follow the
semver policy in `RELEASE.md` (D20): additive content — a new course, a new
stage wave — is a MINOR bump; anything that could strand existing learner
state is a MAJOR bump; defect fixes that touch no contract are PATCH bumps.
History before 0.18.0 lives in the git log and in HANDOFF.md §2, one
paragraph per session.

## [0.20.0] — 2026-11-23

### Added

- The stages 16–20 wave (D22): every one of the thirteen courses grows
  its fourth band — five upper-intermediate stages (City and Directions /
  Money and Services / Nature and the Environment / Opinions and News /
  Future and Dreams), 80 words and 220 exercises per course, the same
  shared skeleton with each course's own drill identity continuing
  (French the pronouns y, en and lui/leur; German the two-way
  prepositions and the passive; Hungarian the potential -hat/-het and
  the conditional -nék; Turkish the dative and ablative cases and keşke
  + -se; Icelandic mér finnst and the myndi/væri conditional; and every
  course's stage 20 adds its third narrative form). Courses now run to
  twenty stages and 313 words; `level_hint: "upper-intermediate"` is the
  fourth band. Additive content, so a MINOR bump per the semver policy
  in RELEASE.md.

## [0.19.0] — 2026-11-23

### Added

- Review session caps and ordering (D21): reviews present their cards
  oldest due date first (ties alphabetical) and cap at 20 cards a session
  by default — a learner returning after weeks gets a sitting-sized queue,
  not a finished course's whole 233-card deck. `termocab review` grows
  `--limit <n>` (another size) and `--no-limit` (every due card, the
  previous behavior). A capped session names the split in its header
  ("— review: 20 of 233 due word(s) —"), and any run that leaves cards
  due ends with "N word(s) still due — run 'termocab review <lang>' to
  continue."

## [0.18.0] — 2026-11-04

### Added

- The release process (D20): tag-driven releases — the owner pushes
  `vX.Y.Z`, the Release workflow (`.github/workflows/release.yml`) verifies
  the tag (fmt, clippy `-D warnings`, tests, release build, publish
  dry-run), publishes the crate to crates.io, and attaches release binaries
  (Linux and macOS) to the GitHub Release. `RELEASE.md` documents the
  checklist, the semver-for-course-content policy and recovery moves.
- Publish-ready crate metadata (`readme`, `keywords`, `categories`) — the
  crate is now `cargo publish`-ready, and `cargo install termocab` becomes
  the one-line install at the first tagged release (the owner's go-live
  checklist is in `RELEASE.md` § Setup).
- This changelog.
