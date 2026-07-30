# 136 — ICE fix: dispositioned array splat to `echo`

DG hit a compiler panic:

```
$ sash -c 'a=(*.out); echo @{a:?}'
ICE: ArraySplat (@a) must be expanded by the argv builder, not lowered as
a scalar value   (codegen/values.rs:61)
```

## Root cause

DD#69 §3 rejects any array splat to `echo` (flatten hazard) with SB5030 —
but the check only recognized the *bare* `@a` splat. A **dispositioned**
splat `@{a:?}` / `@{a:-WORD}` is encoded by the parser as a
`ParameterExpansion` with operator `arrvals_abort` / `arrvals_default`
(the `arrvals_*` family), which `is_named_array_splat` did **not** match.
So `echo @{a:?}` slipped past SB5030, `echo` (an inline builtin) doesn't run
the external argv-builder that expands splats, and the `ArraySplat` value
reached the scalar `lower_value` path — whose "unreachable" guard panicked.

`is_splat_form` (the SB5026 quoted-splat predicate) had the identical gap.

## Fix

Unified the splat-operator recognition in one helper,
`validate.rs::is_arrvals_splat_op` (matches `arrvals` / `arrkeys` /
`arrvals_*`), used by both `is_named_array_splat` and `is_splat_form` — so
the two predicates can't drift again. `echo @{a:?}` / `echo @{a:-w}` are now
SB5030 (clean), not an ICE. External splats (`/bin/echo @{a:?}`) are
unaffected (correctly accepted — DD#69 §2). Validation-only; the codegen
`values.rs` panic stays as a defense-in-depth invariant, now unreachable.

Specs: `glob_splat.sh` gs.14 (`echo @{a:?}` → SB5030) + gs.15
(`echo @{a:-w}` → SB5030), all 3 modes. native+c+janet 0 regressions;
codegen_tests unchanged (182/74, stash-verified).

## Found while fixing (filed, not fixed)

`bug-quoted-array-splat-echo-literal` — a *quoted* array splat to echo
(`echo "@a"`, `echo "@{a:?}"`) compiles and emits the **literal text**
`@a` / `@{a:?}` at runtime (bash: `x y`). Separate, pre-existing,
silent-wrong-output (not an ICE); affects bare quoted `"@a"` too, so it is
not disposition-specific. Awaiting DG's reject-vs-expand call.
