title: "DD#125 and resolve.rs say `in` and `do` lex as words outside their position; they lex as keywords everywhere"
status: open
priority: P4
kind: doc
depends: []
gates:
  - "DD#125's \"Contextual keywords are included\" and the doc comment on `KEYWORDS` (sash/src/resolve.rs) describe the lexer and parser as they are"
  - "no DD, comment or spec repeats the old claim"

## What

Found 2026-10-27, while writing DD#68's F1+F4 addendum (journal 537), which
cites this task.

Two places explain why `int in=5` used to compile:

- DD#125 ("Contextual keywords are included", lines 54-61): "`in` and `do`
  lex as ordinary words everywhere except their one syntactic position, so
  `int in=5` and `for in in a b` used to compile."
- The doc comment on `KEYWORDS` (sash/src/resolve.rs:192-198): most keywords
  are "already unusable as a variable name because the lexer turns them into
  a non-`Word` token, so `int if=5` fails to parse", but "`in` and `do` lex
  as `Tok::Word` everywhere except their one position".

Neither is true now, and neither was when 2d44023 wrote it (2026-10-09):

- The lexer has given every keyword its own token in every position since
  the hand-written parser's first commit, 824e357 (2026-05-18). That
  includes `in` and `do`. The table is `classify_word_token`
  (sash/src/parse/lexer.rs), split out of the word scanner by 82b9f6a; it
  maps 22 words, from `if` to `END`, and depends on no position.
- `expect_word` (sash/src/parse/parser.rs, the `Tok::In | Tok::Do | …` arm)
  accepts eight keyword tokens where a name is expected: `in`, `do`, `done`,
  `then`, `fi`, `else`, `elif` and `esac`. That is why `int in=5` compiled
  before DD#125, and why it now reaches validation's SB5092. It was already
  so at 2d44023^ (parser.rs:3283 there).

So the closing and continuing keywords reach SB5092, while the opening ones
fail to parse, with a generic SB4014:

| Id | Program | Result, on 8e28c61 and with F1+F4 |
|---|---|---|
| a | `int in=5` | SB5092 "`in` is a name the compiler already understands, so it cannot also be a variable (DD#125)" |
| b | `for in in a b; do echo "$in"; done` | SB5092 "… cannot also be a `for` variable" |
| c | `str do=x` | SB5092 |
| e | `int fi=5` | SB5092 |
| g | `int esac=5` | SB5092 |
| d | `int while=5` | SB4014 "expected variable name in declaration" |
| f | `int if=5` | SB4014 "expected variable name in declaration" |

The outcome DD#125 wants holds: none of these compiles. Only the stated
reason is wrong, and resolve.rs's "fails to parse" covers the opening
keywords only.

## Fix

Correct both texts: every keyword is its own token everywhere; `expect_word`
accepts the eight listed above as names, so they reach SB5092; the others
fail to parse with SB4014. Whether the opening keywords should also reach
SB5092, so that every reserved word gets the same explanation, is a question
for DG, not part of this fix.

Probes: `files/port-f1f4-probes.tar.gz`, directory `lexkw/` (a-g).
