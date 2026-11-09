---
title: "Question: statement separators and declarations; conditional-block policy settled"
status: open
priority: P4
kind: question
depends: []
gates:
  - "DG rules on the remaining separator/declaration questions; DD53/DD71 record them; Q3's DD95 policy is already settled"
  - "every program in the tables prints what its ruling says or is rejected with a diagnostic naming the rule, in both backends"
  - "sp8 and sp9 get a diagnostic that names the actual problem, not the variable name"
  - "the citations of this task in `at_declaration_end` (sash/src/parse/parser.rs) and in the comments of errors.sh 71.9 and csfx.19 are rewritten to the ruling"
---

## What

**Policy boundary, 2026-11-09:** general command groups are retired.
Both `cmd && { a; b }` and `cmd || { a; b }` remain solely as their
corresponding `if` shorthands (DD70/DD95, journal 596).
`retire-command-groups` owns enforcement, including `return { ... }`
and groups accidentally split off after another statement.
Q1/Q2's non-group separator/declaration issues remain separate. A group
refusal cannot prove the ordinary `s=x cmd` split was fixed.

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

The original question compared declaration splitting with sp1. Group
retirement now settles sp1/sp8/sp9 as refusals, with advice about the
block rather than the valid variable name. Separation before an
otherwise legal non-group statement remains Q1/Q2's question; d2's
invalid names belong to `bug-declaration-names-unvalidated`.

### Q3. Settled: conditional-block sugar, not command-list grouping

DG's 2026-10-30 group retirement and 2026-11-09 symmetric shorthand
choice resolve this question. The old measurements below are retained,
not rerun. bc.g4 remains as `if ! /bin/false { echo a; echo b; }`;
bc.g5 and g4b require group refusals. g4a and g4c are conditional
bodies whose inner lists are separate, just as in an explicit `if`.
DD95 still forbids a mixed list; it does not forbid different lists
inside a body. Neither old option was selected as framed.

DD#95: "There is no grouping construct for command lists, and none is
wanted: a list that needs grouping is asking to be an `if` or a dispatch",
and "`( … )` is SB4014 and `{ …; }` is SB4027 there". Historical results:

| Id | Program | Result |
|---|---|---|
| bc.g4 | `/bin/false \|\| { echo a; echo b; }` | `a`, `b` |
| bc.g5 | `{ echo z }` | `z` |
| g4a | `/bin/false \|\| { /bin/true && echo y }` | `y` |
| g4c | `/bin/true && { /bin/false \|\| echo y }` | `y` |
| g4b | `{ /bin/false \|\| /bin/true } && echo y` | SB4023 1:1 |

bc.g4's old comment called a group the only multi-statement spelling.
The accepted contract now names the explicit `if` counterpart.
The earlier reading of g4c as mixed-list grouping is superseded: its
braces delimit an `if` body, so its operators are in separate lists.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Options

- **Q1 (a) reject a word that follows a complete statement on the same
  line** (a statement ends at a newline, `;` or an operator). For d3, point
  to `/usr/bin/env` (DD#53). Group-only bc.g6 already belongs to the
  retirement task; retain non-group controls for the split. Recommended.
- **Q1 (b) keep the split for otherwise legal statements**, and record
  the prefix-assignment-looking d3 in DD53. This option now concerns
  legal non-group followers only; it cannot preserve sp1/sp5/bc.g6's
  retired groups.
- **`return {`** (rt1, rt2), `return ! { ... }` and rt3's split-off
  group are settled refusals under `retire-command-groups`, not part
  of the non-group result-use enforcement in `q-compound-as-condition`.
- **Q2's non-group declaration separation** follows Q1. sp8/sp9's
  groups now require rejection either way, with advice about the
  forbidden block rather than the valid variable name. Their parser
  handling belongs with group retirement; d2's invalid declared names
  retain their separate bug owner.
- **Q3 is settled.** The rejected alternatives were retaining general
  grouping, or deleting the multi-statement shorthand altogether.
  Conditional-block sugar preserves the useful spelling without either.

Probes: `files/port-f1f4-probes.tar.gz`, directories `bk/` (sp1, sp5, sp8,
sp9), `sq2/` (d1–d3), `bl/` (g4a–g4c, cs19) and `wr2/` (rt1 is `rpt`, rt2
is `rpt8` with its two-line twin `rpt4`, rt3 is `rpt3`, and `rpt7`).
