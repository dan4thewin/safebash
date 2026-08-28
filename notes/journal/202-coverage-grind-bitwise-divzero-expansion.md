# 202 — Coverage grind: binary bitwise, div/mod-by-zero, dead `Expr::expansion`

Continuation of the coverage grind. Three concrete, verified wins banked
before pivoting the "account for every uncovered line" directive to a
function-level frame (see next journal).

## cov.65–67 — binary bitwise in arithmetic (native + c)

`$(( a & b ))`, `$(( a | b ))`, `$(( a ^ b ))` with `a=6 b=3` →
`2 / 7 / 5`. These `emit_arith_op` arms (`BitAnd`/`BitOr`/`BitXor`) were
reachable but unexercised by any spec. Added native+c; janet skipped
(coverage excludes the janet backend).

## cov.68–69 — runtime divide/modulo by zero (native)

`$(( 5 / x ))` and `$(( 5 % x ))` with `x=0`: output `0`, stderr contains
`division by 0`, rc 0 (DD#55 runtime-abort channel, not a compile error —
the divisor is only known at runtime). Native-only (`rerr` assertion on
the runtime diagnostic). Exercises the codegen guard that traps a
zero divisor before the `sdiv`/`srem`.

## Dead `Expr::expansion` removed

`Expr::expansion(name, span)` (ir.rs) — a `SimpleExpansion` constructor
shorthand — had **zero callers** (calls table + repo grep). `pub fn` dead
code fires no compiler warning, so it sat invisible. Removed; build clean;
full 3-mode sweep 0 native/c regressions. Verified-dead per the
DG-established method (remove → run suite → green = safe).

## const-fold-command-name → wontfix (DG)

DG: "discard the const-fold-command-name, that's gpp territory." A command
head computed from a `const` binding is a source-preprocessing concern, not
a compiler special-case. A command head stays a literal path; any
variable/expansion head is SB5011. Closed wontfix.

## Verification

`SPEC_MODES="native c janet" bash spec/run.sh`; latest-run-per-test diff vs
`git show HEAD:spec.db`: **0 passing→failing, 0 gone, 8 new passing**
(cov.65–69 native, cov.65–67 c). Native failing count unchanged from HEAD
(pre-existing baseline).
