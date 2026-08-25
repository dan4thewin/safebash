# 182 — Drop `$@` (a `$`-sigil that splats)

DG (reviewing the DD#81 wrap-up) caught that `$@` shouldn't exist: it's a
`$`-sigil form that *splats*, contradicting DD#75 §4 ("`@` is the sole splat;
`$` doesn't splat"). `$1`/`$#` are fine — genuinely scalar (one arg, a count),
so their `$` sigil is consistent; `$*` was already removed. Only `$@` broke the
pattern.

## The record was inconsistent

DD#75 §4 rejects "paste-from-bash friendliness" and makes `@a` the sole splat,
but §5's table + line 172 kept `$@` as *sugar* for `@args` — the very thing §4
rejects. And the impl was half-baked: in-function `$@` unquoted splatted, but
`"$@"` read **empty**. So `$@` never really landed. My DD#81 note had framed the
empty-`$@` as a "desugar gap to fix" — wrong: `$@` should be *dropped*, not
repaired.

## Change

`validate::check_dollar_at` walks function bodies and rejects `$@` / `${@}`
(**SB5045** → "use `@args` (the splat), or `$(join args)` for a joined string").
Top-level `$@` stays SB5024 (function-local → `$(argv)`; `@args` doesn't exist
at top level either). `$1`/`$#` untouched. A `$( … )` inside a function is
walked in its arg scope.

DD#75 §5 + line 172 corrected: `$@` joins `$*` as *removed → `@args`*, not sugar.
DD#81 note corrected.

## `"@foo"` — kept a literal (DG)

We considered rejecting `"@foo"` (a quoted splat) too, but a quoted `@` is a
plausible legitimate literal (`"user@host"`, `"@mention"`), so sash keeps
`"@foo"` as a plain literal — no false positives. Bare `@a` in a *scalar*
context is already SB5025 (→ `$(join a)`). The quoted case is the **converter's**
job: when translating bash `"$@"` it must not emit `"@args"` (a silent literal)
— it emits `@args` / `$(join args)`, or warns. Filed
`converter-dollar-at-migration`.

## Specs / results

functions.sh 81.14 (`$@` → SB5045); cf.f11 rewritten to `$(join args)` (joined,
native+c — janet's `_args` model doesn't expose the `args` name), cf.f12 to
`@args` (splat, all modes). Full `SPEC_MODES="native c janet"` sweep vs
`03b66a1`: 0 regressions, native gate 0, 3 new passing.
