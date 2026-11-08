---
title: "Reject a non-boolean/non-exit-status construct used in a boolean context (DD#70 dual)"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`x=hi && echo $x` is a clean compile error (an assignment is a value binding, not a boolean/status), NOT silently empty"
  - "`if x=foo; then …; fi`, `while x=foo; do …`, `x=foo || y`, `! x=foo` all rejected with the same SB code"
  - "declarations in a boolean context rejected too (`if int x=5; then`)"
  - "a real status producer is still accepted in a boolean context: commands/pipelines (`if grep -q x f; then`), `[[ … ]]`, `(( … ))`, function calls"
  - "the dispatch `[[ … { } ]]` statement in a boolean context keeps its existing SB4028; codes stay adjacent in the SB401x family; native+c+janet; specs added"
---

# Reject non-status constructs in boolean contexts

This is the historical July implementation record. DD88/DD101 later
introduced capture-condition exceptions. DD55's 2026-11-08 ruling now
also rejects explicitly discarded statuses in consumer positions;
`reject-discarded-status-consumers` owns that pending extension, not this
completed task.

DG 2026-07-04: "the reject should apply to any non-boolean/non-exit-status
used in a boolean context." This is the **dual** of the DD#70 must-consume
rule — DD#70 says a boolean/status *producer* must be consumed; this says a
boolean *context* must be fed a producer. Same type discipline, other
direction.

## Symptom (the assignment case that surfaced it)

```
$ sash -c 'x=hi && echo $x' -o /tmp/b; /tmp/b
              # prints nothing — the assignment is silently lost across &&
```

`x=hi && echo $x` should not silently do nothing. Under DD#71 (assignments
are value bindings, not connectable commands) it has no status to gate on,
so it is a **compile error**.

## Rule

A **boolean context** consumes a boolean (from `[[ … ]]`, `(( … ))`,
`match`) or an exit-status (from a command/pipeline/function call). The
boolean contexts are:

- `if` / `elif` / `while` / `until` conditions,
- the **left** operand of `&&` / `||`,
- the operand of `!`.

A construct that is **neither** a boolean nor a status producer — an
**assignment** or **typed declaration** (value bindings, DD#71) — used in
any of these positions is a compile error directing the user to sequence it
(`x=foo; if …`) instead.

## Implementation

The validator already threads the context: `validate_stmt_ctx(stmt, diags,
in_condition)` (`sash/src/validate.rs:74`), with `validate_if`/
`validate_while` passing `in_condition=true` for conditions and
`forwards_condition` propagating through `Pipeline`/`Suppressed`/
`Redirected`/`List`. Extend:

1. Treat the **left of `&&`/`||`** and the **operand of `!`** as condition
   contexts (today a top-level `x=hi && …` is validated with
   `in_condition=false`, so it slips through). This likely means inspecting
   `StmtKind::List` operands rather than blanket-forwarding.
2. In `validate_assignment_stmt` (already has `in_condition`; currently only
   emits SB5007 for arithmetic-in-condition) add: `in_condition` +
   non-arith assignment → new SB code. Mirror for `validate_declaration`.
3. New SB code in the SB401x must-consume family (next free; keep adjacent
   to SB4016–4019 / SB4028 per DD#70). One code covers assignment and
   declaration; message: "an assignment/declaration is a value binding, not
   a boolean or exit status — sequence it (`x=foo; if …`) instead of using
   it as a condition."

## Watch out

- Do **not** reject genuine status producers: a command/pipeline
  (`if grep -q x f; then`), `[[ … ]]`, `(( … ))` or a status-function call.
  The former inclusion of a suppressed command is superseded by DD55's
  2026-11-08 consumer rule. A partially suppressed pipeline remains valid;
  an all-suppressed one does not.
- The dispatch `[[ … { } ]]` statement already rejects as a condition
  (SB4028); leave that, keep codes adjacent.
- Confirm `for`/`case`-style bodies (context-reset) are unaffected.
- The original rejection of `x=$(cmd)` as a condition was superseded by
  DD88/DD101's scoped capture-condition forms. Preserve those handlers;
  an explicit inner `-` cannot discard the status they consume.

## Refs

- `notes/decisions/DD70-error-taxonomy-A-B.md` (the model this extends)
- `notes/decisions/DD71-operand-words-value-bindings.md` (assignments =
  value bindings, which licenses the reject)
