title: DD#91 — the `:!` taint-off operator (a per-use bless-without-predicate)
status: accepted

Settled with DG (2026-09-04). `:!` is a new disposition operator: **`:~` (elide)
plus the taint guard disabled**. `${x:!}` / `$x:!` / `@a:!` / `@{a:!}` use a
tainted value *without* a `bless` predicate — a sanctioned per-use taint bypass.

## Motivation

DG: "a new operand like `$foo:!` which is equivalent to `:~` but also disables
the taint guard — I see it as a compromise between full bless use and the
`--no-taint` full abnegation."

The taint model (DD#24) has two poles: `bless V [[…]]` cleans a variable but
demands a validating predicate; `--no-taint` (DD#82) disables *all* taint
checking program-wide. `:!` is the middle ground — a **predicate-less bless at a
single use site**: the programmer vouches for this one value.

## Semantics

- **`:!` = `:~` + taint-off.** It **elides** when the value is the identity
  element (empty → zero tokens) and yields the value otherwise, exactly like
  `:~`; additionally the value is treated as **clean**, so a tainted subject is
  used without `bless` (no SB5020) and the result **does not propagate taint**
  (`array a=(${x:!} lit)` leaves `a` clean).
- **Wordless**, like `:~`: `${x:!foo}` is SB4051 (scalar), `@{a:!WORD}` is SB5033
  (array).
- **Elidable-position-only**, like `:~`: valid only where a token may disappear
  — a command argument or an array-literal element. A non-elidable position
  (e.g. a scalar assignment `z=${x:!}`) is **SB5046**. (So `:!` is *not* a way to
  assign a trusted value to a scalar; use `bless` for that. This mirrors `:~`.)
- Works for **scalar** (`${x:!}` / `$x:!`) and **array splat** (`@{a:!}` /
  `@a:!`), **braced and brace-less** (DD#90). `@a:!` bypasses the bless-all
  requirement on a tainted array.
- Does **not** bypass unrelated checks: an undeclared subject still trips the
  typo guard (SB5031) exactly as `:~`/`:-` do (all dispositions suppress it
  equally — `:!` is not special there).

## Implementation

Scalar `:!` lowers to `SsaValue::ElideEmpty { source, taint_off: true }` (the
`:~` spelling sets `taint_off: false`); array `@a:!` uses the `arrvals_trust`
operator → `ArraySplatDisposition { taint_off: true, … }`. The taint bypass is
applied in **both** taint passes so they agree:

- CST (authoritative, `validate.rs`): `check_param_expansion_taint` returns
  early for `:!` / `arrvals_trust`; `expr_uses_tainted` returns `false` (clean
  propagation); `check_elide_pexp` treats `:!` like `:~` (SB5046 outside a list).
- SSA backstop (`ssa_taint.rs`, defense-in-depth): `ElideEmpty { taint_off }` and
  `ArraySplat` with a trusting disposition classify **Clean**. (The backstop does
  not yet check these argument positions — task `ssa-taint-full-backstop` — so
  the flags are forward-compatible correctness; the CST gate enforces the bypass
  today.)

Parser: `split_operator` gains `:!`; `parse_splat_op` maps `@…:!` →
`arrvals_trust`; the brace-less lexer (`try_braceless_disposition`) accepts `:!`
wordless. Backends: native/c go through the SSA `ArraySplat`/`ElideEmpty`
(taint_off ignored at codegen — the emitted splat is identical to a wordless
`arrvals_default`); janet's `janet_splat_splice` gains an `arrvals_trust` arm.

No new SB code (reuses SB4051 / SB5033 / SB5046).

## Audit surface

`:!` is a deliberate validator bypass — the adversarial audit's highest-priority
target (AGENTS.md). It is inherently greppable in source. Rather than record it
in `--manifest`, DG asked for a broader **generated audit report of every
bless / taint-bypass site** (task `audit-taint-bypass-report`): the predicate
blesses, every `:!`, `--no-taint*` usage, and the external taint sources — so a
reviewer audits all bypasses without grepping.

## See also

- DD#24 taint model; DD#82 `--no-taint` full bypass; DD#84 `:~` elide; DD#90
  brace-less dispositions; DD#69 array-splat dispositions.
- Specs: `taint.sh` 91.1–91.11 (bypass + controls + rejections), `var_modifiers.sh`
  vm.bl13–15 (`$( )` operands). journal 243.
- Tasks: `audit-taint-bypass-report`, `janet-positional-default-present`.
