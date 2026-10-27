---
title: "BUG: a parse error inside a block also reports SB4072 at the block's closing `}`"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "every program under What and Cause reports its own diagnostics and nothing at a block's closing `}` (k4 reports SB4020 alone)"
  - "a genuinely stray `}` (one that closes nothing) keeps SB4072 (z1, z2)"
  - "specs pin v1, w2 and cs1 in both backends, asserting that SB4072 is absent"
  - "an error before a block's `{` (h1, c13, c22, m4, r7, r8) reports nothing at that block's `}`; skw.20 and cnd.22-cnd.24 assert it"
---

## What

When a statement inside a block (a function, `if`, or `while` body) has a
parse error, the compiler reports that error and then SB4072 at the closing
`}` of the block that held the error. Until F1+F4 (journal 537) the message
read "a `}` here closes nothing — no block is open. bash would keep it
literal; quote it for a literal brace (`'a}b'`)". It now reads "a `}` here
closes nothing — a `}` ends the innermost open block only if that block is a
`{ … }` one, and never right after a `|`, `&&`, `||` or `!` that still needs
a command. For a literal brace, quote it (`'a}b'`)". Either way it is false:
the `}` does close a block.

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
same diagnostics on the base of the port (1309385). The cause is traced
below.

F1+F4 (journal 537) added parse errors that cascade the same way. On the base
of F1+F4 (8e28c61), each of these programs reported one SB5011 and no SB4072:

| Id | Program | First diagnostic | SB4072 at | On 8e28c61 |
|---|---|---|---|---|
| cs1 | `fn f { - ! x; }` / `f` | SB4019 at 1:8 ("`-` cannot suppress a `!`-negated command …") | 1:15 | SB5011 at 1:12, "command 'x' not found" |
| cs2 | `fn f { - str y=$(/bin/false); echo "[$y]" }` / `f` | SB4014 at 1:8 ("`-` suppresses a command, and a declaration is not one …") | 1:43 | SB5011 at 1:8, "command '-' not found" |

ds1, cs2's body at top level (`- str y=$(/bin/false)` / `echo "[$y]"`),
reports SB4014 at 1:1 alone.

## Cause (traced 2026-10-27, during F1+F4)

A parse error inside a block unwinds out of the whole top-level statement, and
parsing resumes at top level:

- `parse_body_until` (parser.rs:448-461) parses a block's statements and
  returns the first error with `?` (458). The error unwinds through the `fn`,
  `if` or `while` that holds the block.
- `parse_top_level_item` (215-242) records the error and calls `recover`
  (239). `recover` (250-263) skips up to and including the next newline or
  `;`, and the top-level loop resumes there.
- The rest of the block is then parsed as top-level statements. When the
  block's `}` comes up as a statement, `dispatch_statement`'s `Tok::RBrace`
  arm (337-343) reports SB4072.

That accounts for the non-cascades under What, and for a third:

- w1's SB4047 does not unwind. `unrecognized_operator_diag` (4088) runs under
  `check_rejected`, which `parse_top_level_item` calls (234) only after the
  whole statement has parsed.
- v3 has no `}` after its error.
- k1 is v1 without the `;` before the `}`: `fn f { s=""; echo ${s:-$#} }` /
  `f a b` reports SB4070 at 1:19 alone. Nothing separates the error from the
  `}`, so `recover` skips the `}` with the rest of the line. Whether the false
  SB4072 appears depends on that `;`.

One error can cascade more than once, and the rest of the block is still
checked, but as top-level statements:

| Id | Program | Diagnostics |
|---|---|---|
| k4 | `if /bin/true {` / `  x=$(( 1 + ))` / `  echo "$x"` / `} else {` / `  echo b` / `}` | SB4020 at 2:5; SB4072 at 4:1 and again at 6:1 |
| k3 | `fn f {` / `  x=$(( 1 + ))` / `  echo "$x"` / `  )` / `}` / `f` | SB4020 at 2:5; SB4014 at 4:3, "expected expression" (the stray `)`); SB4072 at 5:1 |

A `}` that closes nothing keeps SB4072, including one after an error at top
level. z1, `echo ${}` / `}`, reports SB4047 at 1:6 and SB4072 at 2:1. z2,
`x=$(( 1 + ))` / `}` / `echo "$x"`, reports SB4020 at 1:3 and SB4072 at 2:1.

Every row this append adds (cs1, cs2, ds1, k1, k3, k4, z1, z2) was run on
the current tree and on 8e28c61, in both backends; only cs1, cs2 and ds1
differ between the two.

## A second shape: an error before the block opens (F1+F4, 2026-10-27)

The error need not be inside the block. When it comes on the line that opens
one, before its `{`, the statement unwinds with the `{` still ahead, `recover`
skips the `{` with the rest of the line, and the block's own `}` is left to
the top level:

| Id | Program | First diagnostic | SB4072 at | On 8e28c61 |
|---|---|---|---|---|
| h1 | `if $(( 1 + )) {` / `  echo a` / `}` | SB4020 at 1:4 | 3:1 | the same |
| h3 | `while $(( 1 + )) {` / `  break` / `}` | SB4020 at 1:7 | 3:1 | the same |
| c13 | `if { echo a; }; then echo hi; fi` (cov.13) | SB4027 at 1:4 | 1:14 | SB4027 at 1:1, then the same |
| c22 | `if /bin/true {` / `  echo a` / `} elif {` / `  echo b` / `}` (cnd.22) | SB4027 at 3:8 | 5:1 | SB4027 at 1:1, then the same |
| c24 | `echo x` / `if {` / `  echo b` / `}` (cnd.24) | SB4027 at 2:4 | 4:1 | SB4027 at 2:1, then the same |
| m4 | `if /bin/true {` / `  /bin/echo a` / `}` / `else {` / `  /bin/echo b` / `}` (skw.20) | SB4072 at 4:1, the `else` | 6:1 | SB5011 at 4:1 alone |
| r7 | m4 with `elif /bin/false {` on line 4 | SB4072 at 4:1, the `elif` | 6:1 | SB5011 at 4:1 alone |
| r8 | `if … } elif … } else { … } else { … }`, the second `else` at 7:3 | SB4072 at 7:3 | 9:1 | SB5011 at 7:3 alone |

The first five are pre-existing; only SB4027's position moved (F1+F4 reports
it at the `{`). cnd.23 is c13's shape on a second line, and diag-elif's p1
and p3 are c22's. m4, r7 and r8 are new: on 8e28c61 a clause keyword at a
statement head was a command name, and its SB5011 was the only diagnostic.
F1+F4 made the keyword SB4072 (skw.5, skw.6, skw.20), and `recover` then
leaves the clause's `}` behind like any other. Every row was run on both
binaries in native mode, and m4, r7 and r8 in C too. skw.20 and
cnd.22-cnd.24 leave this SB4072 unasserted and cite this task.

## Fix

Recommendation (made during D1, not yet ruled): when the parser recovers
from an error inside a block, it should resume inside that block, so the
block's `}` still closes it. Only a `}` that closes nothing should get
SB4072.

Two ways to do it:

- (a) Recover inside the block. `parse_body_until` records a statement's
  error instead of returning it, then skips (counting braces, so a nested
  block's `}` is not taken for its own) to the next terminator or to its own
  `}`, and carries on. The block still closes, and later errors in it are
  still found (k3's `)`). The recorded errors need somewhere to go: a sink
  threaded through every function that parses a block, or one kept on the
  `Scanner`, as its fatal already is (`scanner_fatal`, read at 147).
- (b) Brace-aware `recover`. After an error, skip to the end of the
  statement it unwound out of, counting braces and including any `else` arm
  (k4), so none of its `}` is left over. Smaller, but it skips the rest of
  the statement, so k3's second error goes unreported until the first is
  fixed.

The recommendation above is (a). With either, k1 and v1 must give the same
diagnostics: whether a `;` comes before the `}` must stop mattering.

F1+F4 adds a third option, and the second shape changes the recommendation:

- (c) Extend the guard F1+F4 put on stray keywords. `stray_block_keyword`
  (parser.rs:383) reports nothing once `Scanner::has_recovered` is set, so a
  `fi` or `done` that a recovery left behind is not reported a second time
  (skw.29). Giving `}` the same test is a one-line change and silences every
  row in this file, but z1 and z2 would lose their SB4072 until their first
  error is fixed, which the second gate forbids.

(a) cannot reach the second shape: its error comes before the block opens, so
`parse_body_until` never sees it. (b) can, by counting braces from the error
to the end of the statement, provided it starts from the depth at which the
statement began (in the first shape the error is already inside the block).
So the recommendation is now (b), which fixes both shapes and keeps z1 and
z2; (a) can follow it, to report a block's later errors (k3). DG has not
ruled.

Found during the batch-531 port (step D1, while measuring the SB4070 shapes
in the D1 corpus). Filed under DG's rule for the port that questions and
uncovered cases become backlog tasks. Traced during F1+F4 (journal 537),
whose new errors at a `-` (SB4019, SB4014) cascade the same way.

Probes: `files/port-f1f4-probes.tar.gz`, directories `bl/` (cs1, cs2), `bk/`
(ds1), `cz/` (k1, k3, k4, z1, z2, w1), `kw2/` (h1, h3, r7, r8), `kw5/` (m4)
and `sk/` (c13, c22, c24).
