# 124 — integer literal range & bit-patterns (Policy A)

Session 2026-06-27 (continued). DG ratified the disposition of
out-of-range integer literals and several DD#25 follow-ups. This entry
covers the literal-range implementation.

## DG rulings this round

1. **Left-shift overflow**: NO trap — shifts are bit manipulation, not
   arithmetic. Already correct in codegen (plain `ishl`). Recorded in
   `bug-runtime-arith-overflow-wraps` as resolved-by-design.
2. **Const-fold overflow "essentially free?"**: investigated → **No**.
   The fold already detects overflow (`checked_*`→`None`,
   transform/expr_handlers.rs) but immediately discards it into a runtime
   `ArithBinOp`. Surfacing it as a compile error needs a mutable
   diagnostics sink threaded through the entire `&VersionState`-immutable
   arith recursion (`arith_to_ssa`/`binop_to_ssa`/`unaryop_to_ssa`), or a
   new constant-evaluator in the validator (which does not fold today).
   Per DG's conditional (runtime sufficient unless free), **runtime trap
   stands**. All-literal overflow like `(( 2**63 + 1 ))` already traps
   correctly at runtime.
3. **Glob question** (`DECISION-glob-splat-disposition-word`): not
   actually pending — DG had already resolved it 2026-06-21 (the
   `@{a:-WORD}`/`@{a:?WORD}` external-splat disposition). plan.md
   corrected.
4. **Out-of-range literal → Policy A** (below).

## Policy A — out-of-range integer literals (DG)

Probing found the bug was broader than the task captured: **six** silent
`0` miscompiles, not one. Policy A (bash-faithful):

| literal                       | sash before | sash now (Policy A) | bash |
|-------------------------------|-------------|---------------------|------|
| `$(( 9223372036854775808 ))`  | 0           | **SB4026** (MAX+1)  | -9.2e18 (wraps) |
| `$(( 99999…(huge) ))`         | 0           | **SB4026**          | wraps |
| `int x=99999…(huge)`          | 0           | **SB4026**          | error |
| `$(( -9223372036854775808 ))` | 0           | **-9223372036854775808** (i64::MIN) | i64::MIN |
| `$(( 0xFFFFFFFFFFFFFFFF ))`   | 0           | **-1** (bit-pattern)| -1 |
| `$(( 0x8000000000000000 ))`   | 0           | **i64::MIN**        | i64::MIN |
| `$(( 0x1FFFFFFFFFFFFFFFF ))`  | 0           | **SB4026** (>64 bit)| wraps |

Principle: **decimal** is an arithmetic magnitude (out of range → compile
error); **hex/octal** are 64-bit bit-patterns (reinterpret u64→i64;
consistent with ruling #1 that bits ≠ arithmetic). i64::MIN is a valid
value and is made expressible in arith via a unary-neg peephole.

## Implementation

- `sash/src/parse/arith.rs`:
  - new `ATok::BadNum(String)` (raw text retained for the diagnostic).
  - `scan_decimal_literal`: `parse::<i64>()` → `BadNum` on overflow.
  - `scan_hex_literal` / `scan_octal_literal`: `u64::from_str_radix(..) as
    i64`; overflow (or no digits, e.g. `0x`, `08`) → `BadNum`.
  - `parse_prefix`: a consumed `BadNum` → `arith_range_error` (SB4026).
  - `parse_unary`: i64::MIN peephole — `Neg` of
    `BadNum("9223372036854775808")` folds to `Literal(i64::MIN)`.
  - new `arith_range_error` helper (SB4026).
- `sash/src/validate.rs`: `validate_declaration` rejects an out-of-range
  decimal literal assigned to an `int`/`const` var (SB4026,
  `int_literal_out_of_range`). The non-arith path keeps i64::MIN working
  (the `-` is part of the literal there).

New diagnostic code **SB4026** "integer literal out of range".

## Based literals — deferred (new bug)

While speccing I found `$(( 16#ff ))` → 16 and `$(( 2#1010 ))` → 2: the
`#` is eaten as a comment before `scan_based_literal` is ever reached, so
based literals drop their digits entirely (silent miscompile). This is
orthogonal to range/overflow; filed `bug-arith-based-literal-hash-comment`
(the based-literal range handling — checked u64 accumulation, base 2..=64,
digit<base — rides along with that fix). Based cases were therefore
excluded from spec section 26.

## Verification

- spec/arithmetic.sh `plan` 44→54; section 26 = 10 new cases.
  native 10/10, c 10/10, janet 7/10 (+3 skipped i64-boundary).
- Native gate: **0 failing** (545 passing). Authoritative spec.db diff vs
  HEAD: failing set **identical** (c=58, janet=106) — zero regressions.
  (The run.sh tally line read 60/107; that count is the known-misleading
  one — the latest-run-per-test diff is the truth.)
- Parser unit tests 12/12. Clean release build, no warnings.
