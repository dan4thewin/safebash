title: "Bug: SB4025's capture advice says every rejected capture is an intrinsic or a concatenation"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "every still-rejected row of the first table gets advice that describes its own RHS; v4 and v5 retain their intrinsic/concatenation explanation"
  - "k8, sa5, v6 and v13 (a typed declaration) say DD#88's rule: a typed declaration is not a capture condition, so assign without the type"
  - "DG rules on Q1 and Q2; a4 and b1 then agree, and v2 and cv1 agree with DG's answer to Q2 in both sash and sash-convert"
  - "specs pin the advice of each row by its distinguishing text, not the bare code, in both backends"

## Current producer scope

DD88's November 12 rules admit otherwise-legal pipelines, including function
filters, and direct named predicates. Do not advise that every non-external
stage or every function capture is forbidden. `predicate-capture-conditions`
owns the direct-predicate extension and its coupled advice; void/integer
results and invalid RHS shapes retain appropriate refusals. Direct file-read
handling is separately owned by `recoverable-file-failures`.

The tables below are October 27 observations, not current acceptance gates.
Q1/Q2 about inner negation and a lone quoted RHS remain separate; this
extension does not choose their answers.

## Found

2026-10-27, while porting F1+F4 (journal 537). It is **pre-existing** for the
assignment rows: every assignment result is the same at `8e28c61` (before
F1+F4) and with F1+F4, in both backends. The typed-declaration rows (k8, sa5,
v6, v13) gave SB4014 "expected variable name in declaration" at `8e28c61`.
F1+F4's B28 fix stops a declaration at `&&`, `||`, `|`, `&` and a header's
`{`, so they now reach the SB4025 that DD#88 asks for, and the wrong advice
with it.

Every row gets the same advice:

> … a capture-condition branches on an exit status — a single external
> command (`if x=$(cmd) { … }`) or `$(getenv NAME)`, … This RHS isn't one —
> it captures a sash intrinsic/builtin whose result is a pure function of its
> arguments and so cannot fail, or concatenates `$(…)` with other text. Bind
> it, then test: `x=…; if [[ … ]]`.

It is wrong for:

| Id | Program | What the RHS actually is |
|---|---|---|
| k8 | `str s=$(/bin/echo hi) && /bin/echo ok` | a typed declaration of a single external capture (SB4025 1:1) |
| sa5 | `if str s=$(/bin/echo hi) { … }` | the same in an `if` (1:4) |
| v6 | `if int n=$(/bin/echo 3) { … }` | the same, `int` (1:4) |
| v13 | `if ! str s=$(/bin/false) { … }` | the same under `!` (1:6) |
| v1 | `if x=$(/bin/echo a; /bin/echo b) { … }` | a multi-statement capture, which DD#88 excludes by name |
| v7 | `if x=$({ /bin/echo hi }) { … }` | a group of one external command |
| v2 | `if x="$(/bin/echo hi)" { … }` | a lone quoted capture (Q2) |
| a4 | `if x=$(! /bin/false) { … }` | a negated external command (Q1) |
| v8 | `if x=$(echo hi \| /bin/cat) { … }` | a pipeline with one builtin stage: half right, since `/bin/cat` can fail |

The advice is right for v4 `if x=$(strlen abc)` (an intrinsic) and v5
`if x=a$(/bin/echo hi)` (a concatenation). v3 `if x=$(f)`, a user function,
got its own transform message under the former blanket function exclusion.
Its present verdict must follow the actual function result kind.

These work (all four builds): k8u `s=$(/bin/echo hi) && /bin/echo ok` prints
`ok`, `hi`; sa5u `if s=$(/bin/echo hi)` prints `hi`; v14
`if x=$(/bin/false 2>/dev/null)` takes the else branch; v12
`if ! x=$(/bin/false)` takes the then branch; b1
`if x=$(/bin/echo a | ! /bin/grep -q zzz)` prints `y` and b3 (`-q a`) prints
`n`. v17 shows that `x="$(cmd)"` and `x=$(cmd)` bind the same value.

Probes: `files/port-f1f4-probes.tar.gz`, directories `fin2/` (k8, sa5, a4),
`fin3/` (k8u, sa5u, a4p), `neg/` (b1, b3) and `adv/` (v1–v17, cv1).

## Cause (read, not yet confirmed by a fix)

`nonstatus_in_condition_diag` (sash/src/validate.rs) has one capture advice.
Its callers `validate_declaration_stmt` and `validate_assignment_stmt` pass
only whether the RHS holds a capture, not why `is_capture_condition_rhs`
refused it, and a typed declaration never asks `is_capture_condition_rhs` at
all. F1+F4 did not modify these functions, so this goes to the backlog under
DG's rule for the port (fix a known bug in a function the port modifies; file
the others).

## Q1. `$(! cmd)` is refused, while `$(a | ! b)` is a capture condition

`! cmd` is parsed as a one-command pipeline with its negation flag set
(`parse_negated`), and `is_capture_condition_rhs` accepts a pipeline only
when it has more than one stage (`p.commands.len() > 1`), so a4 falls to its
`_ => false` arm. In b1 the `!` is a mark on the second stage, which
`stmt_is_external_command` peels through `stage_body()`, so b1 and b3
compile and branch on the inverted status. `bug-negated-reader-loop-false-sb4073`
has the same root: a one-command `!` pipeline is treated as a pipeline.

- **(a) Accept `$(! cmd)`** and branch on the inverted status, as b1 does.
  Recommended: a single command is a one-stage pipeline, and this is what b1
  already does.
- **(b) Refuse `!` inside a capture condition**, pointing to the outer
  `if ! x=$(cmd)` that DD#88 documents. b1 and b3, which compile today, become
  errors.

## Q2. A lone quoted capture, `if x="$(cmd)"`

DD#88's scope admits "a single external `$(cmd)`" and excludes a
concatenation. v2 is neither: it is the same value as `$(cmd)` (v17), and it
is refused. bash scripts often write it, and sash-convert passes bash's
`if x="$(/bin/echo hi)"; then` through unchanged (cv1), so the converted
program does not compile.

- **(a) Accept it** as the same RHS as `$(cmd)`. Recommended: an assignment
  does not split, so the quotes change nothing, and converted scripts compile.
- **(b) Keep refusing it**, with advice to drop the quotes, and have
  sash-convert drop them.

## Fix direction

Have `is_capture_condition_rhs` say why it refused (or classify the RHS once,
in a helper both callers use), and give `nonstatus_in_condition_diag` one
advice per reason: typed declaration, multi-statement or compound capture,
pipeline with an ineligible stage, intrinsic, concatenation, and whatever Q1
and Q2 leave refused. A typed declaration should say DD#88's rule ("untyped
only"): assign without the type, `if s=$(cmd) { … }`, or bind first and
test after.

If Q1 is answered (a), the transform's `try_capture_condition`
(sash/src/transform/stmt_handlers.rs) must lower the one-command `!`
pipeline too; run a4 in both backends.
