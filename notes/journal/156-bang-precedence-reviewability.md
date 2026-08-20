---
title: "`!` precedence + no-pipeline-operand (reviewability)"
date: 2026-08-19
---

# `!` binds a single unit; `! pipeline` rejected (SB4040)

The remaining `!` work from DD#70, plus a design refinement DG drove.

## The design conversation

DG asked why `a | b && c` is rejected (SB4023, "ambiguous") but `! a | b` is
allowed. Digging in: **bash's precedence resolves both unambiguously** — the
"ambiguous" wording was never about semantics. DG's real principle: *"sash
needs to be easier to review than bash"* — the ambiguity is from the
**non-expert reviewer's** perspective, who shouldn't have to know operator-vs-
pipe precedence. Under that lens `! a | b` is *just as* unreviewable as
`a | b && c`, so allowing it was inconsistent.

**DG's call:** reject `!` on a pipeline (new code), escape hatch = wrap the
pipeline in a function and negate the function.

## What shipped

- **`! X && Y` → `(!X) && Y`** (was wrongly SB4018). `parse_negated` now parses
  a **single unit** via `dispatch_statement` (which folds neither `|` nor `&&`)
  and returns `!X`, letting the caller's `maybe_wrap_redirects` fold the
  trailing `&&`/`||`. `!` binds only its immediate command/`[[ ]]`/`(( ))`.
- **`! pipeline` → SB4040** (new). `parse_negated` rejects a trailing `|` (or an
  already-folded multi-stage pipeline). Message points to the predicate-function
  escape hatch: `f() { if a | b; then return 0; fi; return 1 }` then `if ! f`.
- **`- ! X` → SB4019** (was the garbage SB4014). A direct parse-time reject in
  `parse_suppressed` (routing it through the transform gave SB4018, since the
  suppression is redistributed onto the inner stage). Being parse-time, it now
  reaches **all** backends, not just native.

## Principle (now in DD#55)

A pipeline may be a **whole** `if`/`while` condition, but **never** inline with
`!`, `&&`, or `||` — those require precedence knowledge sash refuses to demand
of a reviewer. `!` binds a single unit. SB4023 (`pipeline &&/||`) and SB4040
(`! pipeline`) are the same family.

## Verification

spec errors.sh bng.1-6 (runtime `(!X) && Y` + escape hatch native-only — c/janet
`!` runtime is a separate backend gap; the SB4040/SB4019 parse rejects run all
modes). 3-mode sweep 0 regressions. DD#55 rationale reframed from "ambiguous" to
reviewability; DD#70 addendum supersedes its "`! pipeline` is fine" line.

## Deferred

c/janet `!` runtime parity (emit_c `__negate_test_result` TODO; janet negated-
pipeline arm) — unchanged by this, still filed under `c-janet-dd55-abort-channel`.
