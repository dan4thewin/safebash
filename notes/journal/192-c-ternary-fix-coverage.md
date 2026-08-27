# 192 — c-backend ternary fix + ternary coverage

Continuing the coverage grind (journal 190/191). The fully-uncovered-function
scan flagged native `values.rs::lower_ternary_value` (44 lines) — the arith
ternary `cond ? a : b`. It works in native, so it's reachable-untested. Adding
coverage exposed a c-backend bug and a native bug.

## c-backend bug: ternary emitted empty output

`echo $(( x>0 ? 1 : 2 ))` in the c backend printed **nothing** (native: 1).
Root cause: `emit_c::lower_value_as_str` had no `SsaValue::Ternary` arm, so it
fell to the `_ => /* TODO */ ("", 0)` fallback and emitted a zero-length write.
`lower_value_as_int` was missing it too.

Fix: route `Ternary` through `lower_value_as_int` (added to the ArithBinOp arm
in `lower_value_as_str`), and lower it there via a new `emit_ternary_as_int_c`
helper — an **if/else into a fresh temp**, not a C `?:`. The if/else matters:
`lower_value_as_int` for checked ops (`+`/`-`/`*`, div/mod) emits *statements*
(overflow/zero guards); a C `?:` would place both branches' guards before the
select, running them unconditionally. The if/else emits each branch's
statements inside its own block, so guards run lazily — matching bash.

Now c matches native for all simple cases (1/2/10/20, nested) and matches
*bash* for the lazy case.

## native bug (filed, not fixed): ternary is eager

`int x=0; echo $(( x>0 ? 10/x : 42 ))`:
- bash → `42` (lazy; `10/x` never evaluated)
- native → `42` on stdout **plus** `bash: division by 0` on stderr
- c (after this fix) → `42`, no warning

Native's `lower_ternary_value` is straight-line (cc=1): it computes both
branches and selects, so the untaken branch's div-by-zero (or overflow trap)
still fires — an oracle divergence for side-effecting untaken branches. This
needs real control flow (brif + phi, like `if`). Filed
`native-ternary-eager-eval` (P2). Pre-existing and separate from the c
empty-output bug.

## Coverage

coverage.sh cov.49-56 (native+c): both branches, expression branches,
falsy/truthy conditions, nested ternary, assignment context, `!=` condition.
Deliberately side-effect-free branches so native==c==bash until the native
laziness bug is fixed. +8 native, +8 c; 0 regressions.
