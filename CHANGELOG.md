# Changelog

All notable changes to termocab are documented here. Versions follow the
semver policy in `RELEASE.md` (D20): additive content — a new course, a new
stage wave — is a MINOR bump; anything that could strand existing learner
state is a MAJOR bump; defect fixes that touch no contract are PATCH bumps.
History before 0.18.0 lives in the git log and in HANDOFF.md §2, one
paragraph per session.

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
