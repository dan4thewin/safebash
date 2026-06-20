---
title: "[[ ]] value operands → SsaValue: drop right_is_quoted + lower_subscript (DD#62)"
status: ready
priority: P2
kind: refactor
depends: [DD62-test-expr-simplification]
subsumes: dynamic-map-subscript-in-test
gates:
  - "TDD: spec tests for every miss below are written first and red before any code change"
  - "every [[ ]] VALUE operand is lowered as SsaValue (LHS of all ops; RHS of ==/!=/</>; unary -n/-z/-f/-d/… args; case subject) — only =~/case PATTERN operands remain raw text"
  - "right_is_quoted is deleted (zero callers)"
  - "lower_subscript (codegen/values.rs, test-path) is deleted (zero callers); subscripts in [[ ]] go through lower_subscript_value"
  - "==/!= are exact-only (no glob); =~ quoted RHS is regex-grouping (not literal); -v is removed"
  - "native spec clean (only pre-existing cf.c7/cf.c8); rust suite no net regression (--test-threads=1); complexity gate green"
---

# [[ ]] value operands → SsaValue (DD#62)

> **Progress (2026-06-19).** Sub-units landed: (1) **`==`/`!=` exact-only**
> (native `emit_eq`/`emit_ne` drop the `fnmatch` glob branch — c was
> already exact; this aligns them), `emit_regex` always regex (dead
> quoted-literal branch removed), `right_is_quoted` deleted (zero callers);
> spec `wordsplit.sh` 62.1–62.4. (2) **`-v` removed** — rejected in
> validation with **SB5028** (`check_unsupported_test_ops`, extends the
> SB5019 numeric-op check), `emit_var_set_test` deleted; spec 62.5/62.6,
> SUBSET.md row. Verified zero net regression both times. **Still TODO:**
> the operand→`SsaValue` refactor + delete `lower_subscript` (section A).
> **C/D** (`=~` quoted/spaced RHS + the `a[[:space:]]b` `]]`-lexing bug)
> **folds into DD#63** — that DD reworks `=~` into a `match` command and
> rewrites `parse_regex_rhs`, so editing it here would be throwaway (the
> task already says "land `=~`-desugaring first"). The `&&/||` chain (F)
> was a separate root cause — **fixed** (`bug-andor-right-associative`),
> and surfaced a janet/c backend chain bug, **filed**
> (`bug-andor-backend-chain-status`).

Implements DD#62 and, as a consequence, retires the duplicate test-path
lowering subsystem. Spec-first per repo convention: **write all the
spec tests below first, confirm red, then change code.**

## Why (one paragraph)

`[[ ]]` operands reach codegen as raw `ir::Expr` (transform clones the
whole `TestExpr` into `SsaInst::Test`), so the test path re-lowers them
via `lower_expr_value`/`lower_subscript`, a duplicate of the SSA path's
`lower_value`/`lower_subscript_value`. The duplicate `lower_subscript`
guesses map-vs-indexed key type from index *syntax* (it lacks the
`assoc_arrays` info the SSA path threads) — the root of the
`[[ ${m[$k]} ]]` bug. The only reason value operands must stay raw IR is
`right_is_quoted`; DD#62 removes every feature that needs it
(`==`/`!=` glob, the dead `=~` quoted-literal branch). Then value
operands can be lowered to `SsaValue`, and `lower_subscript` dies.

## Spec tests to write first (every miss from the 2026-06-17 discussion)

Place subscript/test cases in `spec/types.sh` (DD#33 home) and
test-operator cases alongside existing `[[ ]]` tests; pick IDs in the
local scheme and bump `plan=`. Use the no-ICE assertion pattern for
rejections (`[[ $rc != 0 && -z $out && $err == *SBxxxx* ]]`).

### A. Subscripts as test value-operands (the refactor's payoff)

- **A1** `map m=([a]=1 [b]=2); k=b; if [[ ${m[$k]} = 2 ]]; then echo y; fi` → `y`
  (fixes `dynamic-map-subscript-in-test`: map dynamic key, not an int lookup).
- **A2** `map m=([a]=1); k=a; if [[ ${m[$k]} = 9 ]]; then echo y; else echo n; fi` → `n`.
- **A3** indexed regression: `array a=(10 20); i=1; if [[ ${a[$i]} = 20 ]]; then echo y; fi` → `y`.
- **A4** subscript as unary operand: `map m=([a]=x); k=a; if [[ -n ${m[$k]} ]]; then echo y; fi` → `y`.
- **A5** subscript on RHS: `array a=(5 6); i=0; if [[ 5 = ${a[$i]} ]]; then echo y; fi` → `y`.

### B. `==`/`!=` exact-only (DD#62 #1 — behavior change)

- **B1** `if [[ abc == abc ]]; then echo y; fi` → `y` (exact still works).
- **B2** `if [[ abc == ab* ]]; then echo glob; else echo exact; fi` → `exact`
  (unquoted RHS is NO LONGER a glob — literal three chars `ab*`).
- **B3** `if [[ "ab*" == ab* ]]; then echo y; fi` → `y` (both literal `ab*`).
- **B4** `!=` mirror: `if [[ abc != ab* ]]; then echo y; fi` → `y`.
- **B5** pattern matching still available via regex: `if [[ abc =~ ^ab ]]; then echo y; fi` → `y`.

### C. `=~` quoting = grouping, not literal (DD#62 #2)

- **C1** quoted regex compiles + is regex: `x=axb; if [[ $x =~ "a.b" ]]; then echo y; fi` → `y`
  (`.` is a regex metachar even though quoted — currently SB4014).
- **C2** space in regex via quoting: `x="a b"; if [[ $x =~ "a b" ]]; then echo y; fi` → `y`.
- **C3** single-quote parity: `x="a b"; if [[ $x =~ 'a b' ]]; then echo y; fi` → `y`.
- **C4** quoted regex that should NOT match: `x=axxb; if [[ $x =~ "a.b" ]]; then echo y; else echo n; fi` → `n`.

### D. Regex spaces / POSIX classes (lexer/parse fixes the above expose)

- **D1** POSIX class with trailing `]`: `x="a b"; if [[ $x =~ a[[:space:]]b ]]; then echo y; fi` → `y`
  (today `a[[:space:]]b` mis-terminates the test at the internal `]]`).
- **D2** char class generally: `x=a1b; if [[ $x =~ a[0-9]b ]]; then echo y; fi` → `y` (regression guard).

### E. Remove `-v` (DD#62 #3)

- **E1** `[[ -v x ]]` → unsupported-operator error (rejected, not silently `-n`).
- **E2** confirm the intended replacement: `x=5; if [[ -n $x ]]; then echo y; fi` → `y`.

### F. `&& / ||` chaining after a false test (bug found in discussion)

- **F1** `[[ 1 = 2 ]] && echo A || echo B` → `B`
  (today prints nothing — the 3-term `false && X || Y` chain drops `|| Y`).
- **F2** `[[ 1 = 1 ]] && echo A || echo B` → `A` (regression guard).
- Note: this is `List`/`&&`/`||` terminator lowering, possibly a
  *separate* root cause from the operand refactor — verify; split into
  its own fix if so, but the spec test belongs here since it was found here.

### G. sed `/` delimiter + `\` — MOVED to DD#63 / `regex-interpolation-delimiters`

The regex quoting/interpolation/delimiter/`\` work (and `=~` pattern
interpolation, which is also unimplemented) is owned by DD#63 and the
`regex-interpolation-delimiters` task. This task keeps only the `=~`
*operand-lowering* changes; the `=~` pattern RHS stays raw text here
regardless. Coordinate the `parse_regex_rhs` edits between the two.

## Implementation outline (after specs are red)

1. **Transform**: lower `[[ ]]` *value* operands to `SsaValue` while
   keeping `=~`/`case` *pattern* operands as raw text and the `case`
   `=glob` path intact. (Likely: a `TestExpr` variant carried in
   `SsaInst::Test` whose value leaves are `SsaValue`; or pre-lower
   operands into the instruction.)
2. **Codegen tests.rs**: consume `SsaValue` operands via the SSA path
   (`lower_value`/`lower_subscript_value`). Delete `right_is_quoted`,
   the `is_quoted` branches in `emit_eq`/`emit_ne`/`emit_regex`, and the
   `==`/`!=` `fnmatch` calls (exact-only). `emit_regex` always regex.
3. **Delete `lower_subscript`** (and any now-dead `lower_expr_value`
   value arms) once value operands no longer route through them. Keep
   `sash_fnmatch` (still used by `case`'s `=glob`/`emit_glob`).
4. **Remove `-v`**: drop `emit_var_set_test` + its dispatch; make `-v`
   an unsupported-operator diagnostic.
5. **`=~` quoting / spaces**: teach `parse_regex_rhs` to accept a quoted
   RHS as part of the pattern (grouping), and fix the `]]`-vs-class
   lexing for `a[[:space:]]b` (D1).
6. **`&& / ||` chain (F1)**: investigate the false-then-`||` terminator
   lowering; fix or split out.
7. **sed `/` / `\` + regex interpolation**: owned by DD#63 /
   `regex-interpolation-delimiters` (out of scope here).

## Cross-refs

- **DD#63 synergy:** under the regex-builtin-family model, `[[ $x =~ re ]]`
  desugars to a `match` *command*, so `=~` leaves the `TestExpr` entirely.
  That removes the only pattern operand from the test path — every `[[ ]]`
  operand becomes a value, `emit_regex`/`right_is_quoted` disappear, and
  this refactor gets *simpler*. Coordinate the seam with
  `regex-interpolation-delimiters`; ideally land `=~`-desugaring first.
- Subsumes `dynamic-map-subscript-in-test` (closed by step 3).
- Adjacent pre-existing, separately filed (verify they stay green / get
  swept in): `nonbrace-dynamic-subscript`, `subscript-append-broken`,
  and the arithmetic-subscript / `${!a[@]}` gaps noted there.
- Mirror the same operand changes in `emit_c.rs` (c backend has its own
  `[[ ]]` lowering: emit_c.rs:1040 fnmatch, subscript emit) for parity.

## Section A — concrete refactor plan (scoped 2026-06-19)

Goal: `[[ ]]` value operands reach codegen as **pre-lowered `SsaValue`**
(so subscripts use `assoc_arrays` key-typing via `lower_value` →
`lower_subscript_value`), killing the duplicate `lower_subscript` and the
`[[ ${m[$k]} ]]` bug. The SSA value path already does subscripts right
(`subscript_to_value` sets `is_integer_key = !assoc_arrays.contains`).

**New SSA repr** (`ssa.rs`, near SsaValue:459):
```rust
pub enum SsaTest {
    Unary { op: String, operand: SsaValue },
    Binary { left: SsaValue, op: String, right: SsaTestRhs },
    Not(Box<SsaTest>), And(Box<SsaTest>, Box<SsaTest>),
    Or(Box<SsaTest>, Box<SsaTest>), Paren(Box<SsaTest>),
}
pub enum SsaTestRhs { Value(SsaValue), Pattern(String) } // Pattern = =~ / =glob raw text
```
Change `SsaInst::Test(TestExpr)` → `Test(SsaTest)` (ssa.rs:228) AND the
bless inst `test: TestExpr` (ssa.rs:293) → `SsaTest` (same lowering path).

**Transform** (`transform/expressions.rs` or a new `tests.rs`): add
`pub(super) fn test_to_ssa(test: &TestExpr, state: &VersionState) -> SsaTest`.
Value leaves via `expr_to_value`; for `Binary` right, `=~`/`=glob` →
`SsaTestRhs::Pattern(expr_to_string(right))`, else `Value(expr_to_value)`.
Apply at the 7 creation sites — control_flow.rs:79,715,739;
pipelines.rs:50,104,208,255 — and the bless site. `statements.rs:77`
`TestExpr::Not` → `SsaTest::Not`.

**setup.rs**: rewrite `intern_test_expr` to walk `SsaTest` using
`intern_value` for value leaves and `intern_value(&SsaValue::Literal(p))`
for Pattern leaves. Replace `scan_test_for_vars` body with
`scan_value_for_vars` over the value leaves (Pattern has no vars).

**codegen/tests.rs**: `lower_test_expr` consumes `&SsaTest`; add a
`func_constants` param (dispatch.rs:98 has `ctx.func_constants`). Value
operands → `lower_value`; `=~`/`=glob` Pattern → lower the raw string as a
literal (reuse `lower_literal_value`). emit_eq/ne use two `lower_value`s.
**Delete** `lower_pair`, `lower_expr_value`(values.rs:20)/`lower_subscript`
(values.rs:182) once the test path no longer calls them (check no other
callers — `lower_concat` uses `lower_expr_value`, so it may need rerouting
to `lower_value`, OR keep `lower_expr_value` if still used elsewhere; the
gate is specifically `lower_subscript` deleted).

**Verify**: `[[ ${m[$k]} ]]` spec (task A1–A5 in `spec/types.sh`); full
native+janet+c spec clean; rust suite no net regression; complexity gate.
