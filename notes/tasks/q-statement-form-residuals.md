---
title: "Question: a word after a complete statement, a declaration beside a block, and DD#95's grouping"
status: open
priority: P4
kind: question
depends: []
gates:
  - "DG rules on each question below; each ruling is recorded in the DD it names (DD#53, DD#71 or DD#95)"
  - "every program in the tables prints what its ruling says or is rejected with a diagnostic naming the rule, in both backends"
  - "sp8 and sp9 get a diagnostic that names the actual problem, not the variable name"
  - "the citations of this task in `at_declaration_end` (sash/src/parse/parser.rs) and in the comments of errors.sh 71.9 and csfx.19 are rewritten to the ruling"
---

## What

Found 2026-10-27, while porting F1+F4 (batch-531 B28, journal 537). F1+F4
made a header `{` end a declaration, so `if int n=$(cmd) {` is DD#88's SB4025
(specs 71.6–71.8). It left the statement level alone (71.9). These are the
questions it did not decide. All results are the same at `8e28c61` and with
F1+F4, in both backends.

### Q1. A word after a complete statement silently starts a new one

| Id | Program | Result |
|---|---|---|
| sp1 | `s=x { echo hi }` then `echo "$s"` | `hi`, `x` |
| d3 | `s=x /bin/echo hi` then `echo "$s"` | `hi`, `x` |
| sp5 | `{ /bin/echo z } /bin/echo a` | `z`, `a` |
| bc.g6 | `echo a { echo z }` | `a`, `z` |
| csfx.19 | `{ /bin/echo hi } 2 >/dev/null` | SB5011 1:18 "command '2' not found — use absolute path or define as function" |

bc.g6 is a decision on record (spec/brace.sh: "Pinned so the divergence is
a decision on record rather than an accident"). sp1, d3 and sp5 are not
recorded. d3 is bash's prefix assignment `FOO=bar cmd`, which DD#53 says sash
does not have ("Sash has no prefix-assignment syntax for env vars"; the
converter prepends `/usr/bin/env`, and spec 53.6 pins only that form). sash
runs it as two statements with no diagnostic: a reader expecting bash's
meaning gets a different program.

The split also happens after `return`, and there it deletes code:

| Id | Program | Result |
|---|---|---|
| rt1 | `fn f { return { /bin/true } }` then `if f { echo y } else { echo n }` | SB4037 1:1 "function `f` is used as a predicate but does not `return` on every path" |
| rt2 | `fn f { /bin/true; return { echo x } }` then `f` then `echo "rc=$?"` | `rc=0`; `x` is never printed |
| rt3 | `fn f { return /bin/true { echo x } }` then `if f { echo y } else { echo n }` | SB4037 1:1, same |

In rt1 and rt2, `return` takes no value and the group becomes the next
statement, which no path reaches. rt2 behaves as `return; { echo x }` does
(rpt7), and neither draws a diagnostic. In a predicate the group is the last
statement, so SB4037 fires, but it blames a missing `return`, not the `{`.
DD#74's table of producers has no `{ … }`. The disambiguation in
`parse_return` (`[[`, `((`, `!` or a non-numeric word is a producer; a
numeric word, a `$`-expansion or the end of the statement is a value) does
not say what a `{` is, so the value form takes it as the end. For comparison,
`return if …` is SB4014 (`q-compound-as-condition`, c9), and `return ! { … }`
returns the group's status (bng.23). rt3 is the plain Q1 split after a
producer.

### Q2. A declaration does not split the way an assignment does

| Id | Program | Result |
|---|---|---|
| sp8 | `int n=1 { echo hi }` then `echo "$n"` | SB4014 1:1 "expected variable name in declaration" |
| sp9 | `array a=(x) { echo hi }` then `echo "${a[0]}"` | SB4014 1:1, same |
| d2 | `int n=1 /bin/echo hi` | SB5050 three times: `n`, `/bin/echo` and `hi` are declared as variables |
| d1 | `int n=1 m=2` then `echo "$n$m"` | `12` |

Should a declaration split like sp1, or should sp1 be rejected like sp8? The
sp8/sp9 message is wrong either way: the name `n` is fine, and the problem is
the `{`. d2 is `bug-declaration-names-unvalidated`.

### Q3. DD#95 says there is no grouping construct outside `[[ ]]`

DD#95: "There is no grouping construct for command lists, and none is
wanted: a list that needs grouping is asking to be an `if` or a dispatch",
and "`( … )` is SB4014 and `{ …; }` is SB4027 there". Today:

| Id | Program | Result |
|---|---|---|
| bc.g4 | `/bin/false \|\| { echo a; echo b; }` | `a`, `b` |
| bc.g5 | `{ echo z }` | `z` |
| g4a | `/bin/false \|\| { /bin/true && echo y }` | `y` |
| g4c | `/bin/true && { /bin/false \|\| echo y }` | `y` |
| g4b | `{ /bin/false \|\| /bin/true } && echo y` | SB4023 1:1 |

bc.g4's comment records why the group survives as an `||` right operand: a
multi-statement right-hand side "has no other spelling". DD#95 was not
amended to say so. SB4027 now means DD#68's "a group cannot be a
condition". g4c is the case DD#95 exists for: inside a group, `&&` and `||`
mix as they may not unbraced.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Options

- **Q1 (a) reject a word that follows a complete statement on the same
  line** (a statement ends at a newline, `;` or an operator). For d3, point
  to `/usr/bin/env` (DD#53). Revisit bc.g6 with it. Recommended.
- **Q1 (b) keep the split**, and record it in DD#53 (d3) and DD#68 (sp1,
  sp5) as bc.g6 is recorded. rt3's group then stays unreachable.
- **`return {`** (rt1, rt2) goes with `q-compound-as-condition`'s ruling.
  Under its (A), make it SB4014 as `return if` is (c9): one more token in
  `parse_return_value_form`'s guard. Recommended. Under its (B), make it a
  producer with the group's status, as `return ! { … }` already is
  (bng.23).
- **Q2** follows Q1. Under (a), sp8 and sp9 get Q1's diagnostic. Under (b),
  a declaration splits too, and `at_declaration_end` stops at `{` outside a
  header as well. Either way the message changes.
- **Q3 (a) amend DD#95**: record bc.g4/bc.g5 as the grouping DD#95 allows,
  and decide whether a group may contain a mixed `&&`/`||` (g4c).
  Recommended.
- **Q3 (b) apply DD#95 as written**: reject groups outside `[[ ]]`. That
  costs bc.g4's multi-statement `||` right-hand side, which has no other
  spelling.

Probes: `files/port-f1f4-probes.tar.gz`, directories `bk/` (sp1, sp5, sp8,
sp9), `sq2/` (d1–d3), `bl/` (g4a–g4c, cs19) and `wr2/` (rt1 is `rpt`, rt2
is `rpt8` with its two-line twin `rpt4`, rt3 is `rpt3`, and `rpt7`).
