# 137 — splat is not a scalar; disposition is command-arg-only (DD#69 addendum)

Follow-up to the `echo @{a:?}` ICE (journal 136). DG asked me to review the
notes for prior reasoning before "fixing" more — which corrected a mistake
and grounded the real work.

## The correction (DG's review paid off)

I had filed `bug-quoted-array-splat-echo-literal`: `echo "@a"` /
`echo "@{a:?}"` print the literal `@a` / `@{a:?}`. Reviewing DD#22 + DD#60
and checking bash showed that is **correct, not a bug**:

- bash: `echo "@a"` → `@a`, `echo "${a[@]}"` → `x y`. In sash-native the
  splat sigil is the **unquoted** word `@a`; inside `"…"` a `@` is an
  ordinary literal (`"user@host"`, `"@handle"` — both valid). DD#22's
  "quoting an array splat is a compile error" targets the *bash-style*
  spellings (`"${a[@]}"` / `"$(argv)"`, coerced by sash-convert), not the
  native literal `"@a"`.

So I **retracted** that task. Rejecting `"@a"` would break literal `@` and
diverge from bash for no gain. (DG chose: keep `"@a"` literal.)

## The real gaps (Q2) — a splat is not a scalar

DD#60 restricts the `$(argv)` splat to splat positions (for-in sole source
/ snapshot); everything else is SB5025. That rule was **never extended to
the native `@a` splat**, so these silently misbehaved:

- `x=@a`, `x=@{a:?}` — scalar assignment silently flattened the array into
  a joined string. → now **SB5025** (a splat is not a scalar; use
  `$(join @a)` for a string, `${a[i]}` for one element).
- `for f in @{a:?}` — a dispositioned splat in a for-in header was
  accepted. → now **SB5034** (new): the `@{a:…}` empty-disposition is
  **command-argument-only**; a for-in source is the bare `@a`.

Kept valid: `for f in @a`, `/bin/echo @{a:?}`, `echo "@a"` (literal).
Test/arith splats were already rejected (SB4018 / SB4035, no ICE).

## Implementation

`validate.rs`: new `Assignment` arm in `check_argv_splat_stmt` →
`check_argv_splat_assignment` (SB5025 when the RHS or subscript index
`is_named_array_splat`); `check_for_header_splat` rejects a
`is_disposition_splat` word (SB5034). Helpers `named_splat_scalar_diag`,
`disposition_position_diag`, `is_disposition_splat`. `is_named_array_splat`
already recognizes `arrvals` / `arrkeys` / `arrvals_*` (from the ICE fix),
so dispositioned splats are covered.

DD#69 gained the "command-argument-only" addendum (SB5025 reused, SB5034
new). Specs: glob_splat.sh gs.16 (`x=@a`→SB5025), gs.17 (`x=@{a:?}`→SB5025),
gs.18 (`for f in @{a:?}`→SB5034), gs.19 (`echo "@a"`→literal `@a`). All 3
modes, 0 regressions; codegen_tests unchanged (182/74).
