---
title: "Question: `$-` in a word is literal text — reject it, or keep it?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the question below; the ruling is recorded as a DD addendum (DD#72's addendum of 2026-10-24 covers arithmetic only)"
  - "the ruling is pinned by specs in both backends: the unquoted and double-quoted word, and `[[ $- == *i* ]]`"
  - "if the ruling rejects, every program under Already rejected keeps its diagnostic"
---

## What

Bash's `$-` expands to the shell's option flags: `bash -c 'echo "$-"'`
prints `hBc`, and the same line in a script file prints `hB`. sash has no
`$-`. In arithmetic, a `$` that starts no variable became SB4035 during the
batch-531 port (DD#72's addendum of 2026-10-24). In a word or a
double-quoted string it is still literal text. In both backends:

| Program | Result |
|---|---|
| `echo "[$-]" [$-]` | rc 0, prints `[$-] [$-]` |
| `echo "[%$%]"` | rc 0, prints `[%$%]` |
| `if [[ $- == *i* ]] { echo i }` | SB5086 at 1:7, "this comparison is decided at compile time — both operands of `==` are fixed by the source text; compare a variable instead" |

### Already rejected

Every other spelling, and every other bash special parameter, gets a
diagnostic:

| Program | Result |
|---|---|
| `echo "[${-}]"` | SB4034 at 1:6, "the `-` parameter operator (in ${…}) isn't part of sash — …" |
| `echo "[$(( $- + 1 ))]"` | SB4035 at 1:6, "stray `$` in arithmetic — …" |
| `echo "[$0]"` | SB5017 |
| `echo "[$*]"` | SB5045, "sash has no `$*` — …" |
| `echo "[$#]"` (top level) | SB5024, "$# is function-local — use $(argc) at top level …" |
| `echo "[$_]"` | SB5031, "'$_' is assigned nowhere …" |

So `$-` is the one bash special parameter that a word silently turns into
literal text. Its commonest use, the interactive test `[[ $- == *i* ]]`,
gets a message about comparing two constants instead of one saying that
sash has no `$-`.

sash-convert passes `echo "$-"` through unchanged; `case $- in` gets the
DD#66 MANUAL note.

In an `s///` field DD#63 rules separately: a `$` that no name character
follows is a literal regex anchor. There `$-` is an anchor followed by `-`,
which never matches: `x=a-b; echo "[${x:s/$-/X/}]"` prints `[a-b]`, and the
control `${x:s/-b$/X/}` prints `[aX]`.

## Cause

Both word scanners in `sash/src/parse/lexer.rs` recognise a special
parameter from a fixed set, `@ * # ? ! $` and the digits, which has no `-`:
`Scanner::scan_dollar` (unquoted) and `Scanner::scan_dq_dollar`
(double-quoted). After any other byte, the `$` is kept as literal text and
the byte is left for the next token, so `$-` reaches the program as the two
characters `$-`.

## Open question

- **(a) Reject `$-` in a word.** Both scanners would need to recognise `-`
  as a special parameter, so the validator could reject it in the SB5017
  family ("does not exist in sash"), as it does `$0` and `$RANDOM`.
- **(b) Keep it literal.** Record that a word `$` that starts no variable is
  literal text. DD#64's quoting table already gives `'$'` as the spelling of
  a literal `$`, but nothing requires it. `[[ $- == … ]]` could still get a
  more useful message than SB5086's.
- **(c) Reject every word `$` that starts no variable**, matching the
  arithmetic rule. `echo "[%$%]"` and `echo "cost: $"` would then need
  `'$'`. Not measured: how many specs and converted scripts carry a bare
  `$`. sash-convert would have to rewrite the bare `$`s it passes through.
  Whatever the ruling, DD#63's anchor rule for `s///` fields would stay.

Recommendation (made during F14, not yet ruled): (a). In bash, `$-` never
means the two characters `$-`, so the literal reading is always a
mistranslation, and its commonest use draws a misleading diagnostic. The
reading is at least visible in the output, unlike the arithmetic case,
which is why this is P3.

Found during the batch-531 port (step F14, finding B38). Filed under DG's
rule for the port that questions and uncovered cases become backlog tasks.
