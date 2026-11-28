# Changelog

All notable changes to termocab are documented here. Versions follow the
semver policy in `RELEASE.md` (D20): additive content — a new course, a new
stage wave — is a MINOR bump; anything that could strand existing learner
state is a MAJOR bump; defect fixes that touch no contract are PATCH bumps.
History before 0.18.0 lives in the git log and in HANDOFF.md §2, one
paragraph per session.

## [0.24.0] — 2026-11-28

### Added

- Slovak, the sixteenth course (D26): twenty-five stages on the shared
  five-band skeleton, 393 words, 1098 exercises — and the soft wave,
  the sixth ACCENTED extension a language has forced and the small one
  the large Czech one foreshadowed: three entries (ľ ĺ ŕ), every one of
  them decomposable, none of them closed. Slovak rides the caron entries
  its neighbour Czech just collected for six of its letters (č ď ň š
  ť ž) and the Latin-1 acutes for the rest, so the whole remaining wave
  is the three its neighbour never forced: ľ, the palatalized l of
  ľudia and ľahký — a letter no course before this one spelled — and
  the doubled syllabic sonorants ĺ and ŕ, the long l and r that carry
  a syllable alone (stĺp, kĺb, vŕba, kŕdeľ), something no other
  shipped language asks a letter to do. The course is Slovak through
  and through: bare nouns (no articles, like Polish, Hungarian and
  Czech), the case engine every band drills — v + locative (v malom
  byte, v kuchyni), do + genitive (do mesta), s + instrumental (s
  mliekom, kartou), k + dative (k lekárovi), the position prepositions
  (na križovatke, pri moste) — the gendered past -l/-la with its som
  auxiliary, homonym translations on mesiac (a month, also the Moon),
  list (a letter, also a leaf), hlas (a voice, also a vote), voliť (to
  vote, also to elect), pošta (the post office, also the mail) and
  krajina (a country, also a landscape), and stage 25 closes the arc
  with the past counterfactual (keby som to vedel, urobil by som to
  inak). Additive content, so a MINOR bump per the semver policy in
  RELEASE.md.

## [0.23.0] — 2026-11-25

### Added

- Czech, the fifteenth course (D25): twenty-five stages on the shared
  five-band skeleton, 393 words, 1098 exercises — and the caron wave,
  the fifth ACCENTED extension a language has forced and the largest
  single one: nine entries (č ď ě ň ř š ť ž ů), every one of them
  decomposable, none of them closed. ř — the letter no other language
  spells, and the reason the roadmap put Czech first among the
  table-forcing candidates — folds exactly like ğ did before it: the
  fold is a keyboard kindness, not a phonology claim. ů takes the å
  rule to a new base letter, so Czech's two spellings of the long u,
  ú and ů, both fold to plain u and meet in the loose pass. The course
  is Czech through and through: bare nouns (no articles, like Polish
  and Hungarian), the case engine every band drills — v/na + locative
  (v bytě, na stole), do + genitive for direction (do školy), s +
  instrumental for company and means (s mlékem, vlakem, kartou), k +
  dative for the doctor (k lékaři) — the gendered past -l/-la with
  its jsem auxiliary, homonym translations on kolo (a bicycle, also a
  wheel), byt (an apartment beside the infinitive být), hlas (a vote,
  also a voice), volit (to vote, also to elect), pošta (the post
  office, also the mail) and známka (a grade — its stamp sibling
  travels as poštovní známka to keep every srs key unique), and stage
  25 closes the arc with the past counterfactual (kdybych to věděl,
  udělal bych to jinak). Additive content, so a MINOR bump per the
  semver policy in RELEASE.md.

## [0.22.0] — 2026-11-25

### Added

- Norwegian (Bokmål), the fourteenth course (D24): twenty-five stages
  on the shared five-band skeleton, 393 words, 1098 exercises — and
  **zero engine changes**: å rides the Latin-1 accent fold D9 wrote,
  æ rides the ligature tier it shares with Icelandic and Dutch, and ø
  stays closed exactly as D9 ruled — the ruling's own example word
  was *smør*, and the language it was written for arrives to collect
  it. The course is Bokmål as written in Norway: nouns carry their
  *en*/*ei*/*et* articles (the three-gender drill, richer than
  Swedish's two), the enclitic definite article is blanked at the end
  of the noun with the feminine taking her own *-a* (the ending that
  separates Bokmål from its Scandinavian siblings), *vil*/*kunne* +
  infinitive, the V2 word order after fronted adverbs, homonym
  translations on *time* (an hour, also a lesson), *lære* (to learn,
  also to teach) and *stemme* (to vote, also a voice), and stage 25
  closes the arc with the past counterfactual (*om jeg hadde visst,
  ville jeg ha valgt annerledes*). Additive content, so a MINOR bump
  per the semver policy in RELEASE.md.

## [0.21.0] — 2026-11-24

### Added

- The stages 21–25 wave (D23): every one of the thirteen courses grows
  its fifth band — five advanced stages (Culture and the Arts /
  Science and Technology / Society and Politics / Feelings and Idioms
  / Memories and Reflections), 80 words and 220 exercises per course,
  the same shared skeleton with each course's own drill identity
  continuing (French the subjunctive after *il faut que*, the
  relatives *qui/que* and *ce qui/ce que*, and the past
  counterfactual as its fourth narrative form; German the relative
  clauses and the Konjunktiv II; Polish the instrumental and the
  *gdybym … -łbym* past conditional; Turkish the *-dik* relative
  participle and the *keşke* past; Icelandic the *hefði* of the unmet
  past; Hungarian the *volna* counterfactual; and every course's
  stage 25 closes its narrative arc). Courses now run to twenty-five
  stages and 393 words; `level_hint: "advanced"` is the fifth band.
  Additive content, so a MINOR bump per the semver policy in
  RELEASE.md.

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
