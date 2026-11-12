title: "BUG — a non-simple redirect target expr (`> $(cmd)`, `> ${x:-f}`, …) silently redirects to an EMPTY filename"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`echo hi > $(echo f)` either writes to file `f` (if implemented) or rejects at compile time — never silently redirects to `\"\"`"

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

## Resolution (2026-08-30) — DG: support scalars, reject complex

DG ruled: a redirect target must be a **scalar** the compiler can evaluate to a
filename (`$x`, `${x:-default}`, `$a[i]`, literal/quoted/concat of those);
**reject** a command substitution `$(…)` or arithmetic `$((…))` — the compiler
can't know the resulting filename, so a silent-empty is worse than an error.

Implemented in two parts (native + c; janet has no file redirects — it emits a
no-op comment — so its behavior is unchanged, and the reject is parse-level so it
applies there too):

1. **Reject (parse, SB4049)** — `parse_redirect` now runs
   `reject_nonscalar_redirect_target` on every file-redirect target
   (`>`/`>>`/`<`/`<>`/`&>`, not `<<<` here-strings, not `>&`/`<&` fd-dups which
   keep SB4048). A target that is not a scalar (`redirect_target_is_scalar`
   allowlist: Literal/SingleQuoted/SimpleExpansion/ParameterExpansion/Subscript,
   or a Concat/DoubleQuoted of those) → SB4049.

2. **Support scalar reads (transform spill)** — the ad-hoc redirect-target
   codegen only knew literals / `$x` / concats, so `${x:-f}` (ParameterExpansion)
   and `$a[i]` (Subscript) hit the silent-empty fallthrough. `lower_redirected_stmt`
   now `spill_scalar_redirect_targets`: any target that needs spilling is lowered
   via the normal `expr_to_value` (inheriting default/subscript/string semantics),
   bound to a synthetic `__redir_target_N` scalar, and the redirect rewritten to
   read `$__redir_target_N` — which codegen already handles. No new SSA type; the
   silent-empty `redirect_expr_zero` / `ir_expr_to_ssa_value` fallbacks are now
   unreachable for redirect targets (they remain only on the here-string path).

specs: commands.sh 14.19–14.24 (reject cmdsub/arith all-modes; support
`${x:-d}` default+set, `$a[i]`, `>> $a[i]` native,c). taint.sh 60.41/60.52
rerouted through a scalar (a direct `$(join …)` target is now SB4049 at parse,
before the SB5020 taint check — the array-join taint-at-redirect-target path is
preserved via `x=$(join …); … > $x`). 0 native/c regressions.

Follow-up filed: **bug-herestring-nonscalar-data-empty** — `<<< ${x:-f}` /
`<<< $a[i]` / `<<< $(cmd)` still silently empty (the here-string path was left
untouched, pending a DG ruling on here-string data semantics).
