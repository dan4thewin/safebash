# 203 — Coverage accounting: the measurement is accurate (per-function is the frame)

The pivotal session for DG's "account for every uncovered line" directive. The
headline is a **methodology correction**: line-level "1765 unaccounted" is not
noise to be explained away — the measurement is accurate, and the reliable unit
is the per-function execution count.

## What I got wrong, then right

I had been treating optimized-release line coverage as too noisy to trust, and
hypothesised that even function counts suffered "inlining false-zeros"
(a small function inlined into a covered caller reporting count 0). **Wrong.**

Measured three ways — full `--release`, `-Cinline-threshold=0`, and
`CARGO_PROFILE_RELEASE_OPT_LEVEL=0` — the coverage numbers are **byte-identical**
(regions 3164 missed, lines 1753, functions 80). Source-based LLVM coverage maps
regions through optimization; inlining does not corrupt it. The per-function
`count` from `cargo llvm-cov report --json` is authoritative.

What had fooled the earlier "false positive" framing: a manual probe of a
feature succeeds (`(( x++ ))`→6, `for k in $(keys m)`→keys) and looks like proof
the function ran — but the feature routes through a *different* path than the
never-run function. Statement `(( x++ ))` uses the statement mutation lowering,
not `arith_to_ssa`'s value-arm (`incdec_to_ssa`, count 0). For-in `$(keys m)`
uses the splat path, not `try_native_intrinsic` (`try_keys_intrinsic`, count 0).
A passing feature probe does NOT prove a specific function executed.

## The reliable frame

Function coverage: **95.03% (73/1469 never-run)** — a tractable, trustworthy
list. Every named never-run function has ≥1 caller (none structurally dead), so
each is reachable-in-principle and classified by whether a spec can reach it:
A (untested → add spec), B (shadowed by an upstream reject → dead), C (non-`-c`
CLI only), D (defensive). Full classification in `notes/coverage-accounting.md`.

## Specs added (cov.70-75), per-function count 0→>0 verified

- cov.70 `if (( a, a>0 ))` → `validate_arith_comma_in_condition` (count 2).
- cov.71 `s=~ s/a/b` (two delimiters) → `sed_malformed_err` SB4032 (count 2).
- cov.72 `${s:1:3}` substring → `emit_string_op_expansion` (count 1).
- cov.75 dispatch predicate, all arms return → `case_always_returns` (count 1).
- cov.73 (keys for-in) / cov.74 (compact dispatch) pass but hit adjacent paths;
  their intended targets stayed 0 — comments corrected to describe reality.

lines 92.06%→**92.28%**, functions 94.55%→**95.03%**, 0 native/c/janet
regressions, 10 new passing.

## Two P1 bugs surfaced (the whole point)

Exercising unexamined paths is how bugs surface — proven again:
1. **const-reassign panic** — `const x=1; x=2` → `panic!` ssa.rs:926. (filed)
2. **intrinsic-arity fallthrough** — `$(sub a)`, `$(toupper)` (wrong arg count)
   fall through to `sash_exec` → runtime `bug: bare command passed to sash_exec`.
   Should be a compile-time SB. (filed `bug-intrinsic-arity-fallthrough.md`)

Both are `bug:`/panic invariants reachable from user source — must never
happen. Both await a DG fork decision (A/B/C in the task files).

## Measurement recipe (for next rounds)

`cargo llvm-cov clean --workspace` → `cargo build --release` → native+c specs →
`cargo llvm-cov report --json` (ignore-regex excludes janet/convert/expand/
tests/main/oracle) → `functions[].count==0` is the never-run list. Restore with
`cargo clean -p sash --release && cargo build --release -p sash` (uninstrument).
`rustfilt` demangles the v0 names. Any opt level gives the same answer.
