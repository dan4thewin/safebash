---
title: "BUG: a parse error inside a block also reports SB4072 at the block's closing `}`"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "every program under What reports its first diagnostic and nothing at the block's closing `}`"
  - "a genuinely stray `}` (one that closes nothing) keeps SB4072"
  - "specs pin v1 and w2 in both backends, asserting that SB4072 is absent"
---

## What

When a statement inside a block (a function, `if`, or `while` body) has a
parse error, the compiler reports that error and then SB4072, "a `}` here
closes nothing — no block is open. bash would keep it literal; quote it for a
literal brace (`'a}b'`)", at the closing `}` of the block that held the
error. That second message is false: the `}` does close a block.

| Id | Program | First diagnostic | SB4072 at |
|---|---|---|---|
| v1 | `fn f { s=""; echo ${s:-$#}; }` / `f a b` | SB4070 at 1:19 ("a `:-` default and a `:?` abort message are literals, so a `$` expansion …") | 1:29 |
| v2 | `fn f { s=""; echo $s:-$#; }` / `f a b` | SB4070 at 1:19 | 1:27 |
| v4 | the same as v2, one statement per line | SB4070 at 3:8 | 4:1 |
| v5 | `fn f { s=""; echo $s:-"$x"; }` / `f` | SB4070 at 1:19 | 1:29 |
| w3 | `if [[ -n a ]] { s=""; echo $s:-$x; }` | SB4070 at 1:28 | 1:36 |
| w4 | `while [[ -n a ]] { s=""; echo $s:-$x; break }` | SB4070 at 1:31 | 1:45 |
| w5 | `fn f { echo "${x:-$y}"; }` / `f` | SB4070 at 1:13 | 1:25 |
| w2 | `fn f { x=$(( 1 + )); echo $x; }` / `f` | SB4020 at 1:10 ("arithmetic expression ends where an operand is expected") | 1:31 |

Not every error cascades. w1, `fn f { echo ${}; }`, reports SB4047 alone
(since F14; before it, the program compiled). v3, the same error at top
level, reports SB4070 alone. Pre-existing: all eight cascading rows give the
same diagnostics on the base of the port (1309385). The cause has not been
traced.

## Fix

Recommendation (made during D1, not yet ruled): when the parser recovers
from an error inside a block, it should resume inside that block, so the
block's `}` still closes it. Only a `}` that closes nothing should get
SB4072.

Found during the batch-531 port (step D1, while measuring the SB4070 shapes
in the D1 corpus). Filed under DG's rule for the port that questions and
uncovered cases become backlog tasks.
