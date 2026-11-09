---
title: "Question: may a non-group compound (`if`, `while`, `until`, `for`) supply a consumed status?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the options below; the ruling is a DD#68 addendum"
  - "every remaining non-group program is rejected with a diagnostic naming the rule or prints what the ruling says, in both backends"
  - "the citations of this task in `parse_negated` (sash/src/parse/parser.rs) and in DD#68's 2026-10-26 addendum are rewritten to the ruling"
  - "bng.24 and other still-legal compound cases match the ruling; group-only bng.22/bng.23 migration belongs to retire-command-groups"
---

## Group boundary settled, 2026-11-09

DG retired general command groups and accepted both conditional-block
shorthands: `cmd && { a; b }` is `if cmd { a; b }`, and
`cmd || { a; b }` is `if ! cmd { a; b }` (DD70, journal 596).
`retire-command-groups` owns enforcement, including `! { ... }` and
`return { ... }` refusals. Those are no longer choices in this task.

An allowed right-hand block is the corresponding `if` body, not a
group with its own status. This task still owns whether the remaining
non-group compounds can supply a consumed status; the sugar creates no
exception to whatever rule applies to its explicit `if` form.

## What

The evidence below predates the accepted group retirement and has not
been rerun. DD#68 rejected a group condition (SB4027), and F1+F4 extended
that to a negated group in a header (journal 537). Other compound
commands were accepted as an `if`/`while` condition, as the right
operand of `&&`/`||`, and under `!`. In those positions their status is not
bash's, and a failing command in the body still aborts the program. Outside
a header, a group under `!` was accepted too (see the historical table).

Found 2026-10-27, while porting F1+F4 (journal 537). Pre-existing: the
results below are the same at `8e28c61` (before F1+F4) and with F1+F4, in
both backends.

| Id | Program | sash | bash |
|---|---|---|---|
| c1 | `if if /bin/true { /bin/true } { echo y } else { echo n }` | y | y |
| c2 | `if while /bin/false { /bin/true } { echo y } else { echo n }` | n | y |
| c2b | `int i=0` then `if while (( i < 2 )) { i+=1 } { echo "y$i" } else { echo "n$i" }` | n2 | y2 |
| c3 | `if for x in a { /bin/false "$x" } { echo y } else { echo n }` | aborts: `/bin/false: command failed (exit 1)` | n |
| c3t | c3 with `/bin/true` | y | y |
| c4 | `if ! for x in a b { /bin/false "$x" } \|\| /bin/true { echo y } else { echo n }` | aborts | y |
| c5 | `if ! while /bin/false { /bin/true } { echo y } else { echo n }` | y | n |
| c8 | `/bin/true && if /bin/false { /bin/true }` then `echo after` | after | after |
| c9b | `fn f { return ! if /bin/false { /bin/true } }` then `if f { echo y } else { echo n }` | y | n |
| c10 | `if /bin/true && for x in a { /bin/false "$x" } { echo y } else { echo n }` | aborts | n |

sash's status for a compound is that of the last condition it evaluated.
bash's is that of the last body command, or 0 when no body ran. Body
commands keep their statement-level check, so c3, c4 and c10 abort where bash
takes the else branch.

The same compounds are rejected in neighbouring positions:

| Id | Program | Result |
|---|---|---|
| c9 | `fn f { return if /bin/false { /bin/true } }` | SB4014 1:15 "return: expected a status producer (a command, `[[ … ]]`, or `(( … ))`)" |
| e1 | `if /bin/false { /bin/true } && echo y` | SB4023 1:1 "`&&` / `\|\|` on a compound command isn't allowed — wrap the compound in a function" |
| e2 | `if for x in a { /bin/true } && /bin/true { echo y } else { echo n }` | SB4023 1:4 |
| e3 | `while /bin/false { /bin/true } \|\| echo alt` | SB4023 1:1 |
| c6 | `! while /bin/false { /bin/true }` then `echo after` | SB4018 1:1 |
| c7 | `/bin/echo a \| ! while readline x { /bin/true }` | SB4022 1:17 "a negated `while` isn't allowed as a pipeline's last stage" |

So SB4023 rejects a compound only as a left operand. `!` hides it there
(c4), and a right operand is accepted (c8, c10). `return` rejects a compound
(c9), but `!` hides it there too (c9b). F1+F4 changed only c6 and c7: at
`8e28c61` c6 gave SB4018's old advice, and c7 panicked at
`stmt_handlers.rs:1011` ("a compound command reached a pipeline stage").

Only bng.24 (spec/errors.sh) pins a compound condition as a positive:
`if ! for x in a b { echo $x } || /bin/true { echo y }` prints `a`, `b`, `y`.

### Groups under `!` (historical; now require rejection)

SB4027 covers header conditions only (DD#68's 2026-10-26 addendum). Where no
body follows, a group under `!` is still accepted, as an `&&`/`||` left
operand and as a `return` producer. bng.22 and bng.23 pin g1 and g2 as
positives.

| Id | Program | sash | bash |
|---|---|---|---|
| g1 | `! { /bin/false } && echo y` | y | y |
| g3 | `! { /bin/false; /bin/true } && echo y` then `echo after` | aborts: `/bin/false: command failed (exit 1)` | after |
| g4 | `! { /bin/true; /bin/false } && echo y` then `echo after` | y, after | y, after |
| g2 | `fn f { return ! { /bin/true } }` then `if f { echo y } else { echo n }` | n | — |

Without the `!`:

| Id | Program | Result |
|---|---|---|
| g1t | `{ /bin/false } && echo y` | SB4023 1:1 |
| rt1 | `fn f { return { /bin/true } }` then `if f { echo y } else { echo n }` | SB4037 1:1: the body parses as a bare `return` followed by a separate group (`q-statement-form-residuals`, Q1) |

So `!` hides a group from SB4023 exactly as it hides a loop (c4). A group's
status is its last command's, as in bash, so g1 and g4 agree with bash. Its
earlier commands keep their statement-level check, so g3 aborts where bash
prints `after`, as c3 does. Without the `!`, `return` never sees a group
(rt1), so it cannot reject one as it rejects `if` (c9). bash's `return`
takes no producer, so g2 has no bash result.

A right-hand conditional block is now settled separately:
`/bin/false || { echo a; echo b; }` (bc.g4, spec/brace.sh) is `if`
sugar, not a surviving command group. Q3 of
`q-statement-form-residuals` records that same boundary.

For still-legal compounds, the nested `$?`/latch defect belongs to
`bug-sb5012-sb5094-nested-body-flow`. Under (A) its remaining
negated-compound rows become rejections. Group retirement already
settles its group-only rows; an earlier syntax refusal is not a flow fix.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Options for non-group compounds

- **(A) Reject a compound wherever its status is consumed**: as a condition,
  as either operand of `&&`/`||`, under `!`, and as a `return` producer.
  This is one rule, in the SB4023/SB4027 family. It extends DD#68's reasoning
  that a group is not a condition, and DD#95's that a list needing grouping is
  asking to be an `if`. The advice is the one SB4023 already gives: wrap the
  compound in a function, whose `return` states the status. bng.24 flips.
  Recommended.

  The conditional-block shorthands retain their ordinary `if` meaning;
  they do not supply a separate group's status. General groups and
  their former bng.22/bng.23 positives already belong to the retirement
  task, regardless of which option is selected here.
- **(B) Keep them legal with bash's status** (the last body command, or 0),
  and decide whether body commands in that position are checked. Today they
  abort, as they do at statement level. This does not reopen groups.
- **(C) Keep `if` only.** Reject the loops, and give `if` bash's
  status when no branch runs (c9b differs today).

Probes: `files/port-f1f4-probes.tar.gz`, directories `cq/` (c1–c10, c2b,
c3t, c9b), `sq3/` (e1–e3) and `wr2/` (g1–g4, g1t; rt1 is `rpt`).
