---
title: "janet: a compound `if` condition is emitted as `(if true …)`"
status: deferred
deferred-by: DD104
priority: P1
kind: bug
depends: []
gates:
  - "janet: `if [[ -z x ]] || [[ -z y ]]; then echo yes; else echo no; fi` prints `no` (matches native/c)"
  - "janet: `if [[ -n x ]] && [[ -z y ]]; then echo yes; else echo no; fi` prints `no`"
  - "janet: `if false || false; then echo yes; else echo no; fi` prints `no`"
---

## Found

2026-09-07, adding the DD#93 Part C shape tests (journal 248). Spec
`types.sh` 93.16 (high bytes rejected by `p?`/`g?`) failed **only** in janet.
The operators were not at fault — the emitted janet was:

```janet
(if true
  (do (print "yes") …)
  (do (print "no") …))
```

The whole condition had been replaced by `true`.

## Scope

Not specific to DD#93. The trigger is a **compound (multi-statement) `if`
condition** — any `List` in the condition position. All three reproduce with
pre-existing operators, native/c disagreeing with janet:

| source                                            | native/c | janet |
|---------------------------------------------------|----------|-------|
| `if [[ -z x ]] \|\| [[ -z y ]]; then …`           | `no`     | `yes` |
| `if [[ -n x ]] && [[ -z y ]]; then …`             | `no`     | `yes` |
| `if false \|\| false; then …`                     | `no`     | `yes` |

A **single**-statement condition is correct (`if [[ p? café ]]` → `no`), and
so is `||` *inside* one `[[ ]]` (`if [[ p? café || g? café ]]` → `no`), which
lowers to `TestExpr::Or` rather than a statement list. The same `||` outside
an `if` (`[[ -z x ]] || echo fallback`) is also correct — so it is the `if`
condition path specifically, not `List` lowering in general.

This is a **silent wrong answer**, not a diagnostic: the condition is simply
discarded, so every compound `if` takes the then-branch.

## Consequence for existing specs

Any janet-mode spec whose `if` condition is compound is currently passing
**vacuously** when it expects the then-branch. `types.sh` 77.5
(`if [[ i? $s ]] && [[ i? 0xff ]]; then echo yes; else echo no; fi`,
expecting `yes`) is one such — it cannot fail in janet today. Worth a sweep
for the pattern once the emitter is fixed, since fixing it may surface
genuine janet failures that this bug has been masking.

## Fix sketch

In `emit_janet.rs`, the `if` lowering takes the condition's *last* statement
(or a truthiness of the list) and loses the list semantics. It needs to
lower a `StmtKind::List` condition the way native/c do — evaluate the left,
short-circuit on `&&`/`||`, and use the aggregate status as the condition —
rather than defaulting to `true`. Compare `lower_if` in
`transform/control_flow.rs` for the intended semantics.

## Workaround in the meantime

Express the compound inside a single `[[ ]]` (`[[ a || b ]]`), which is
correct in all three backends. Spec 93.16 is written that way.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
