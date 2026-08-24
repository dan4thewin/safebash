# 174 — eliminate-ice-arms cohort A: join arity + CheckedArithOp + case→bool

Three of the ICE arms from the per-pattern review (journal notes / the review
with DG), each retired by narrowing a type or validating an arity rather than
trusting a convention. Committed in two commits (P8 the bug, P4+P6 the
refactors).

## P8 — `$(join)` arity (a reachable compiler panic)  [commit 3fb3458]

`$(join)` with no args reached `transform::join_intrinsic` and
**panicked the compiler** (`ICE: join requires at least one argument`); >2 args
silently dropped the extras. `validate::check_join_arity` now rejects both
(SB5040 — join takes a collection and an optional separator). The transform
branch is an `unreachable!()` citing the validator. spec/types.sh 33.23-24.

## P4 — `CheckedArithOp` (2 ICE arms)

`emit_checked_arith` (native) and `emit_checked_arith_c` (C) took an `ArithOp`
but only handle Add/Sub/Mul, with `_ => unreachable!("only handles Add/Sub/Mul")`.
New `ssa::CheckedArithOp { Add, Sub, Mul }`; the dispatch site maps each `ArithOp`
variant directly (`ArithOp::Add => emit_checked_arith(CheckedArithOp::Add, …)`),
so both emitters are total matches — the arms are gone. Overflow trap (DD#25)
unchanged.

## P6 — case conversion is a `bool`, not a stringly-typed op (2 ICE arms)

`emit_case_conversion(operator: &str)` (native) and `emit_case_conversion_c`
matched `"^"/"^^"/","/",,"` with `_ => unreachable!()`. But the only caller,
`SsaValue::CaseConvert { upper }`, stringified a **bool** to `"^^"`/`",,"` — and
the first-char `"^"`/`","` arms had no producer at all (DD#65 pruned the
`${var^}` family). Changed both emitters to take `upper: bool` (whole-string
toupper/tolower) — no match, no unreachable, and the stringly round-trip is
gone. Removed the now-dead `toupper_first`/`tolower_first` ExternFuncs fields,
setup declarations, and emit_c externs; filed `remove-dead-case-first-runtime`
for the sash-rt twin (needs a runtime rebuild).

## Where the review stands

Retired this session: P8 (bug), P4 (2 arms), P6 (2 arms). Still open from the
review: P2 (lazy interner, ~5 arms), the P1 Pattern-A `match` conversions, and
keep+cite for the table / P3 / P7 sites where an upstream pass (or storage rule)
genuinely backs the invariant. P1 index-var (Pattern B) + the `$@[i]` ICE were
done in journal 173.

## Verification

P8: full sweep, 6 new passing (33.23-24 ×3), 0 regressions. P4+P6: full
`SPEC_MODES="native c janet"` sweep vs the P8 commit — 0 passing→failing, 0 gone,
native gate 0 (behavior-preserving; overflow trap + toupper/tolower spot-checked).
