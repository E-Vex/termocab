# Releasing termocab

Decision D20 (HANDOFF.md §7, the release-process session, Nov 2026) settled
the two open questions that stood between termocab and crates.io: **who cuts
releases**, and **what a semver guarantee means for course content**. This
document is the working form of that decision — the checklist the owner
follows and the policy every change is measured against.

## The decision in one paragraph

D16 fused the courses into the binary, so the crate and its content ship as
**one artifact with one version number** — the `version` in `Cargo.toml`
covers the engine, the CLI and every embedded course together; there is no
separate content version. Releases are cut by **the owner, with a tag**: the
owner bumps the version, writes the changelog entry, and pushes a `vX.Y.Z`
tag to `main`; everything after that is mechanical and automated by
`.github/workflows/release.yml` — the same gates as CI plus a publish
dry-run, then `cargo publish` to crates.io, then release binaries (Linux and
macOS) attached to a GitHub Release with auto-generated notes. There is no
release calendar: a tag is cut when a content wave lands or a user-facing
fix warrants one.

## Setup (owner, once)

1. Create a crates.io API token: crates.io → Account Settings → API Tokens
   → *New Token*, scoped to `publish-update`, named `termocab-releaser`.
2. Add it as the repository secret **`CARGO_REGISTRY_TOKEN`**:
   GitHub → E-Vex/termocab → Settings → Secrets and variables → Actions.
3. That is the whole setup. Until the secret exists, a tag's `publish` job
   fails fast with a pointer here — nothing is consumed, and re-running the
   job after setting the secret works with the same tag.

## Cutting a release

1. Land the work on `main` and let CI go green — releases are always cut
   from a green tip, never from a branch or a local checkout.
2. Confirm the version in `Cargo.toml` matches what this release should be
   (the rhythm: every landed session bumps MINOR; see the policy below).
   The version bump rides the session's docs commit, as it has since 0.5.0.
3. Write the release's entry in `CHANGELOG.md`.
4. Tag and push the tag:

   ```
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

5. Watch the Release workflow run. It verifies the tag (fmt, clippy
   `-D warnings`, tests, release build, `cargo publish --dry-run`), then
   publishes to crates.io and attaches binaries
   (`termocab-vX.Y.Z-<os>-<arch>`) to the GitHub Release.
6. Sanity-check the result: `cargo install termocab --version X.Y.Z` on a
   machine that never saw the checkout, then `termocab list-langs` — the
   built-in courses must all be there (D16 made the published binary
   self-sufficient; that is the whole point of publishing).

## Semver for course content — the policy

The contract across any two releases within the same MAJOR: **an existing
learner's state keeps working**. Concretely, the guarantees are:

- **Stage ids are append-only.** Progress files store `stage <id>` lines;
  ids are never renumbered, never reassigned, never reused. A new wave
  appends stages 16–20; the stages 1–15 a learner already completed stay
  byte-for-byte what they were.
- **Review state only ever orphans, never breaks.** SRS files key on the
  word itself; a word that later content removes or respells leaves an
  orphan card, and orphan cards are skipped at review time and left in
  place (HANDOFF §9) — never an error, never data loss.
- **Content overrides keep winning.** The lookup order (D2/D16 —
  `$TERMOCAB_CONTENT_DIR`, then the XDG data dir, then `./languages`, then
  the built-ins) is contract; an `add-lang` install keeps shadowing the
  built-in of the same name.
- **Grading only grows.** Folds and accepted answers are added, never
  removed within a MAJOR — anything a release graded correct stays correct,
  with its hint, forever after.
- **The CLI surface only grows.** Verbs, flags and exit codes are added,
  never removed or changed within a MAJOR.

The bump a change earns:

| Bump | A change earns it when… |
|------|------|
| **MAJOR** | It can strand state or flip an accepted answer: a storage-format change, stage renumbering or removal, course or vocabulary removal, removing a fold or an accepted answer, removing a verb or flag, changing an exit code. |
| **MINOR** | Additive everything — a new course (the fourteenth language), a new stage wave appended to existing courses, new words, new accepted answers, a new engine table extension (a new fold), a new verb or flag. This is the wave bump: every landed session bumps MINOR, the rhythm the history already follows. |
| **PATCH** | Defect fixes that touch none of the contract above — engine bugs, broken-pipe-class fixes, prompt or hint typos that add nothing and remove nothing. |

While the version is 0.x the discipline above is a **human contract**, not a
resolver guarantee — `cargo install` pins exact versions, so cargo's 0.x
incompatibility rule never bites users. `1.0.0` is cut when the owner
declares the storage format and the CLI surface stable; nothing in this
document depends on that happening.

## Recovery

- **`publish` failed (secret missing, token expired, registry hiccup):**
  the version number is *not* consumed — a failed publish uploads nothing.
  Fix the cause and re-run the failed job from the same tag.
- **`publish` succeeded, `artifacts` failed:** re-run the artifacts job;
  the crates.io release is already live and immutable.
- **A published version is broken:** it cannot be replaced — crates.io
  never allows re-uploading a version number, and a tag must never be
  deleted to "re-release" it. Fix forward on `main` and cut `X.Y.(Z+1)`;
  `cargo yank` is the emergency brake for a genuinely broken published
  version (it drops out of fresh installs; existing installs keep it).
- **Verify said no:** the tag found a real defect — delete the tag (it
  published nothing), fix on `main`, re-tag. This is the one case where
  deleting a tag is honest.

## Release history

Curated history starts at 0.18.0 in `CHANGELOG.md`; the releases before it
are documented in the git log and HANDOFF.md §2, one paragraph per session.
