---
title: "BUG — a non-simple redirect target expr (`> $(cmd)`, `> ${x:-f}`, …) silently redirects to an EMPTY filename"
status: pending
priority: P2
kind: bug
depends: []
gates:
  - "`echo hi > $(echo f)` either writes to file `f` (if implemented) or rejects at compile time — never silently redirects to `\"\"`"
---

## Symptom (native; same silent-degrade class as the fd-dup bug)

A file-redirect whose TARGET is not one of {Literal, SingleQuoted,
SimpleExpansion, Concat, DoubleQuoted} compiles, then at runtime redirects to an
**empty filename**:

```
$ sash -c 'echo hi > $(echo rf); cat rf'    # → "./tb: : No such file or directory"
```

The target `$(echo rf)` (a `CommandSubstitution`) is dropped; the open target is
`""`. `> $f` (a `SimpleExpansion`) works, so this is specific to the un-handled
expr kinds. `> ${x:-f}` (a `ParameterExpansion` with operator), `> f$((x))`
(rejected SB4030 for a different reason), and any `Arithmetic`/`Subscript`
target are the same class.

## Cause

`codegen/redirects.rs`:
- `lower_ir_redirect_expr` (the tilde/runtime redirect-target path) handles only
  Literal / SingleQuoted / SimpleExpansion / Concat / DoubleQuoted; its `_ =>`
  arm is **`redirect_expr_zero`** (line 346) which returns `(iconst 0, iconst 0)`
  — a zero-ptr, zero-len target → open(""). (This is the never-run function the
  coverage grind surfaced.)
- `ir_expr_to_ssa_value` (line 289, pipeline-stage redirects via
  `emit_stage_redirects`) has the same shape: `_ => SsaValue::Literal("")`.

Both silently degrade to an empty target instead of rejecting or evaluating the
expression.

## Fix (needs a DG scope decision — mirrors the fd-dup call)

Remove the silent-empty fallback. Then, for a non-simple redirect target:
- **Reject-only** (recommended, consistent with the 2026-08-29 fd-dup reject-only
  decision + the "no ambiguity" principle): a redirect target must be a plain
  literal / quoted / `$var` / concat word; `$(cmd)`, `${x:-…}`, `$((…))`, `[i]`
  targets → a clean SBxxxx reject.
- **+ implement** `$(cmd)` (bash's "redirect to the file named by cmd output")
  and/or `${x:OP}` targets as a feature.

Either way the invariant is: a redirect target is fully evaluated or rejected —
never a silent empty filename.

## Found

2026-08-30, coverage grind (correct spec-only measurement): `redirect_expr_zero`
+ `ir_expr_to_ssa_value` were never-run; probing what reaches them surfaced the
silent-empty target.
