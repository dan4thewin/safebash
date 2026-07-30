---
title: "BUG — quoted array splat to `echo` emits literal text (`\"@a\"`/`\"@{a:?}\"` → `@a`/`@{a:?}`)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`array a=(x y); echo \"@a\"` and `echo \"@{a:?}\"` either expand (bash: `x y`) or are rejected (SB5026/SB5030) — never emit the literal text `@a` / `@{a:?}`"
  - "no silent-wrong-output for a quoted array splat in any position"
  - "0 native spec regressions; add spec coverage"
---

# BUG — quoted array splat to `echo` is emitted as literal text

## Symptom (silent wrong output)

```
$ sash -c 'array a=(x y); echo "@a"'     -o z && ./z
@a                                        # WRONG — bash: x y
$ sash -c 'array a=(x y); echo "@{a:?}"' -o z && ./z
@{a:?}                                    # WRONG — bash: x y
```

A **quoted** array splat (`"@a"`, or the dispositioned `"@{a:?}"` /
`"@{a:-w}"`) passed to `echo` compiles cleanly and prints the *literal*
splat text at runtime, instead of expanding the array or being rejected.
Same silent-placeholder class as `bug-multistatement-cmdsub-placeholder`.

Discovered 2026-07-29 while fixing the `echo @{a:?}` **ICE** (the *unquoted*
dispositioned splat — fixed: it is now SB5030, see glob_splat.sh gs.14/15).
This quoted variant is a **separate, pre-existing** bug: it does not ICE,
and it affects the bare quoted `"@a"` too, so it is not disposition-specific.

## Root cause (to confirm)

`echo`'s arguments do not appear to reach the quoted-splat guard
(`validate.rs::argv_splat_self_diag` → `is_splat_form`, SB5026), and the
codegen `echo` path does not expand a splat nested inside a `DoubleQuoted`
part — so it falls through to a literal string emission. Either:

- the `check_argv_splat_expr` walk isn't applied to `echo` args, or
- the `DoubleQuoted([splat])` shape isn't matched by
  `argv_splat_self_diag` (which expects `parts.len()==1 && is_splat_form`).

Note `is_splat_form` / `is_named_array_splat` now share `is_arrvals_splat_op`
(recognizes `arrvals` / `arrkeys` / `arrvals_*`) after the ICE fix, so the
predicate is correct; the gap is in *reaching* it for a quoted splat in
`echo` (and possibly other) positions.

## Decision (DG) — what should a quoted array splat to `echo` be?

Per DD#69 §3 an *unquoted* array splat to `echo` is SB5030 (flatten). A
quoted splat is normally SB5026 (quoted splat). Either diagnostic is
defensible for `echo "@a"`; the invariant that must hold is **no silent
literal output**. Pick SB5026 (quoted-splat, general) or SB5030 (echo
flatten), consistently with the unquoted case.

## Fix sketch

Ensure the quoted-splat detection reaches `echo` args (and audit other
positions), so `"@a"` / `"@{a:?}"` is rejected; and/or make the codegen
`echo` path expand/refuse a `DoubleQuoted` splat rather than emitting it
verbatim. Add specs under `glob_splat.sh` for the quoted forms.
