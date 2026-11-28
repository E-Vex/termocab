# Grading

termocab grades answers the way a patient tutor would: it cares about the word, not about your keyboard layout.

## What is always forgiven

- upper or lower case
- extra whitespace
- punctuation at the edges of an answer, including Spanish `¿` and `¡`
- **missing accents.** An accent-only miss counts as correct, and termocab shows the accented form: `correct — watch the accents: à bientôt`

Alternative correct answers can be listed in the content itself: `accepted_answers` on an exercise, `accepted_translations` on a vocabulary entry (so reviewing *salut* with "hello" is right, not wrong).

## How each alphabet folds

Letters fall in one of two groups: they either **fold** (the plain letter is accepted, and the real one is shown as a hint) or stay **closed** (the plain letter is a different letter, so it is a miss).

| Course | Folds: you type, it counts, hint shown | Stays closed |
| --- | --- | --- |
| French, Spanish, Italian, Portuguese | `perche` → `perché`; `irmao` → `irmão`; `maca` → `maçã`; `voce` → `você`; the *crase*: `a` → `à` | — |
| German | `strasse` → `Straße` (`ß` opens like a ligature) | — |
| Polish | `dziekuje` → `dziękuję`; the stroked `ł` opens like `ß`: `lodz` → `łódź` | — |
| Romanian | `multumesc` → `mulțumesc`; the legacy cedilla spellings `ş` `ţ` count for the standard `ș` `ț` | — |
| Swedish | `atta` → `åtta`; `gar` → `går`; `forsta` → `förstå` | — |
| Hungarian | `no` → `nő`; `ketto` → `kettő`; `konnyu` → `könnyű` | — |
| Turkish | `soguk` → `soğuk`; `kardes` → `kardeş` | dotless `ı`: `hayir` is wrong for `hayır`, never hinted. Two different vowels, not one word spelled twice (the same way `ø` stays closed) |
| Icelandic | `ja` → `já`; `tolva` → `tölva`; `æ` rides the ligature fold: `hae` → `hæ`; letters around a closed one still fold: `þu` → `þú`, `goður` → `góður` | `þ` and `ð`: `godur` is wrong for `góður` |
| Catalan | `gracies` → `gràcies`; `felic` → `feliç`; `raim` → `raïm`; the interpunct folds like `ł`'s stroke: `collegi` → `col·legi`, `pellicula` → `pel·lícula` | the hyphen some typists use: `col-legi` is a miss |
| Dutch | `ĳ` opens to the two letters it spells: `het IJsselmeer` → `het Ĳsselmeer`; `een` → `één` (the one word where the accent does grammar); the trema folds: `de ideeen` → `de ideeën` | the schoolbook `y`: `myn` is a miss for `mijn` |
| Norwegian | `ar` → `år`; `traer` → `trær` (`æ` rides the ligature fold it shares with Icelandic and Dutch); letters around a closed one still fold: `spørsmal` → `spørsmål` | `ø`: `smor` is wrong for `smør` — the closed club's founding example, finally spoken by its own language |
| Czech | `cesky` → `česky`; `dekuji` → `děkuji`; `reka` → `řeka` (the letter no other language spells); `dum` → `dům` (the ring rides the å rule); `zena` → `žena`; both spellings of the long u fold to plain `u`: `urad` → `úřad` | — |
| Slovak | `ludia` → `ľudia` (the palatalized l, in no course before this one); `lahky` → `ľahký`; the syllabic pair folds with the rest: `stlp` → `stĺp`, `vrba` → `vŕba` (the long l and r that carry a syllable alone); the shared carons ride the Czech entries: `skola` → `škola`, `zena` → `žena` | — |

## Why some letters stay closed

Folding an accent is a kindness because it is the same letter with a mark missing. `ı`, `ð`, `þ` and `ø` are different letters, not accented ones, so `hayir` or `godur` spell a different word: a miss, and never hinted. Letters next to a closed one still fold, so `þu` is accepted for `þú`.
