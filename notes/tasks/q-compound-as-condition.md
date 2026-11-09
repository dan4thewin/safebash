---
title: "Reject using `if`, `while`, `until` and `for` statements as result producers"
status: open
priority: P3
kind: work
depends: []
gates:
  - "DD68's no-result rule rejects consumed control-flow statements through conditions, negation, status returns and logical-list decisions in native and C, independently of taint checking"
  - "otherwise legal unconsumed final actions and sequential statement prefixes remain valid; consumption propagates to a list tail only when its result is required"
  - "ordinary inner condition handling, body checks, explicit function returns, capture output and pipeline/reader contracts are preserved"
  - "update parse_negated's task citation and take a current fixture census; changed bng.24 contracts use fresh ids, while group-only bng.22/bng.23 migration retains its group-retirement owner"
  - "focused before-binary and targeted reversion controls prove the result-use guard and its boundaries; the predicted ordinary gate passes"
---

## Decision settled, 2026-11-09

DG rejected consumed compound results, emphasizing that sash separates
expression contexts from statement contexts to make procedural code
easy to reason about. DD68 is authoritative; DD55/DD70/DD74/DD95 and
journal 597 record the consequences. This task keeps its slug and P3
priority as implementation work, not an open policy question.

## Accepted rule

`if`, `while`, `until` and `for` have no result. Neither the final
condition nor the final body command supplies an implicit value or
status; empty/skipped paths do not synthesize success. Brace/keyword
spellings, C-style `for` and the conditional-block shorthands follow
the same statement/result distinction.

Reject consuming the compound's result: a condition, `!`, a status
return producer, or a logical continuation decided by that result.
Additional marks, redirects, conditional paths or body placement do
not make a result appear. General groups and compound suppression
already have separate refusal owners.

Use DD74's actual consumer boundary, not the old option A's overbroad
"either operand" wording. An otherwise legal final action in a
statement-level logical list is not a predicate. Historical c8
(`cmd && if ...`) therefore remains legal; c10's consumed condition
tail does not. A middle clause supplies the following connector's
decision, and the final clause supplies the result when the enclosing
list is itself consumed. A non-final member of an otherwise legal
sequential condition list remains a statement.

Normal conditions inside the statement still supply their own
handling status. Body commands keep their checks, including the last
one, and no new status permission follows the compound. Explicit
returns inside bodies belong to their enclosing function. A function
can perform the work and return an explicit status; this neither
changes return kinds nor adds a compound-statement forwarding form.

An otherwise legal capture may contain control flow and produce
stdout; the capture, not its `if`/loop, supplies the value. Preserve
pipeline aggregate status, reader-loop behavior and function/process
boundaries under DD55/DD122. This rule does not turn every statement
inside a result-producing operation into an expression.

No new diagnostic code or lowering architecture is selected here.
Keep source-form/result classification distinct from internal
sequence/body carriers, including arithmetic and loop desugaring.

## Group boundary settled, 2026-11-09

DG retired general command groups and accepted both conditional-block
shorthands: `cmd && { a; b }` is `if cmd { a; b }`, and
`cmd || { a; b }` is `if ! cmd { a; b }` (DD70, journal 596).
`retire-command-groups` owns enforcement, including `! { ... }` and
`return { ... }` refusals. Those are no longer choices in this task.

An allowed right-hand block is the corresponding `if` body, not a
group with its own status. This task now owns enforcement of the
no-result rule on those `if` statements too. Coordinate with the
group-retirement task so a consumed shorthand does not retain the old
group's implicit status.

## Historical measurements (2026-10-27; not rerun)

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

The consumed-compound rows of `bug-sb5012-sb5094-nested-body-flow`
now require refusals here, while group-only rows belong to retirement.
That task remains open for its still-legal bodies. An earlier syntax
or result-kind refusal does not prove a nested-flow repair.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Implementation evidence

The no-result choice replaces the former alternatives of assigning
last-test/last-body results to every compound, or keeping results only
for `if`. Normal body checking is unchanged. The accepted rule does
not reject an unconsumed statement tail merely because it follows a
connector.

Use real CLI controls in both backends, with and without taint
checking. Pin direct, negated, returned and list-tail consumers,
including an effectful body and a path where no body runs. Pair a
consumed tail with an otherwise legal unconsumed action. Preserve
inner condition/status handling, explicit returns inside bodies,
ordinary captures and reader pipelines.

Measure the current fixtures before predicting the migration. The
historical c8 is a preservation, not a new refusal. c1-c5, c9b, c10
and their variants require refusals; existing c9/e1-e3/c6/c7 refusals
are controls, not evidence that the formerly accepted paths were
guarded. A stage or group diagnostic must not masquerade as proof of
the new result-use guard.

Review bng.24 and any other affected positives at implementation time.
Use fresh ids above the database high-water marks for changed
contracts, and preserve independent body/flow assertions in legal
forms before retiring their old spellings. Prove the new refusal with
before-binary and appropriate reversion controls, then run the gate
with measured predictions. No current impact count is inferred from
these historical tables.

Probes: `files/port-f1f4-probes.tar.gz`, directories `cq/` (c1–c10, c2b,
c3t, c9b), `sq3/` (e1–e3) and `wr2/` (g1–g4, g1t; rt1 is `rpt`).
