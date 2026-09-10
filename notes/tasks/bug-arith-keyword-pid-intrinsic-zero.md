---
title: "bug: pid/random keyword intrinsics as arithmetic operands silently yield 0 (only `len` is wired as an arith int-intrinsic)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`$(( (getpid) ))` / `$(( (getcpid) ))` / `$(( (random) ))` compute the value, not 0"
  - "a keyword pid/random intrinsic as a compound operand (`$(( (getpid) > 0 ))`) computes correctly"
  - "decision: does `$(getpid)` command-substitution work inside `$(( ))` (currently SB4020), or is the `(getpid)` sexpr the only in-arith spelling? (DD#58 sexpr vs command-sub in arith)"
  - "native + c + janet; specs pin the values; no regression"
---

## Symptom

Inside `$(( ))`, the DD#58 intrinsic sexpr forms `(getpid)` / `(getcpid)` /
`(random)` compile but evaluate to **0** (silent-wrong):

```
$(( (getpid) ))        → 0   (should be the pid)
$(( (getpid) > 0 ))    → 0   (should be 1)
```

Separately, the command-sub spelling `$(( $(getpid) ))` is rejected outright
with SB4020 ("unexpected token after arithmetic expression").

## Root cause

`is_arith_int_intrinsic` (transform/expr_handlers.rs) recognises only `"len"`.
Every other name — including `getpid`/`getcpid`/`random` — falls through
`funccall_to_ssa` to `SsaValue::IntFuncCall`, which `inline_int_func` resolves
as a *user* int-function; with no user body it returns `Expr("0")`. So the
sexpr silently yields 0.

## Relation to the fix that surfaced this

The sibling bug — the `$$` / `$?` **sugar** specials yielding 0 as arithmetic
operands — was fixed (native `lower_value_as_int` now computes getpid /
exit-status; commit on `hand-parser`, specs arithmetic.sh 31.20–24). That fix
covers the `DollarVar` path (`$$`, `$?`). This task is the *keyword/sexpr* path
(`(getpid)`, `(getcpid)`, `(random)`), a different mechanism (int-intrinsic
dispatch), so DD#75 §1 equivalence in arithmetic (`$$` ≡ `$(getpid)`) is not
yet complete: after the sugar fix `$(( $$ > 0 ))` → 1 but `$(( (getpid) > 0 ))`
→ 0.

## Approach (sketch)

Add `getpid`/`getcpid`/`random` to the arith int-intrinsic set and evaluate
them in the transform's `try_native_intrinsic` (or a dedicated arm) so both the
int and string codegen paths compute the runtime value (the codegen
`value_handlers` already has a `getpid` arm). Decide the `$(getpid)`
command-sub-in-arith question with DG (keep rejected → sexpr-only, or accept).
`implement-getpid-builtins` is marked done for the *word* context; this is the
arith-operand gap it did not cover.

## SUPERSEDED (2026-08-14)

Superseded by `bug-arith-sexpr-intrinsics-zero`, which is the comprehensive
version: this task named only getpid/getcpid/random, but the empirical sweep
found **every** sash-native intrinsic except `len` is broken in a sexpr — the
int-returning ones silently yield 0, the string/collection ones silently yield 0
instead of a type error, plus the `random` int-typing inconsistency, the
wrong-arity `sash: bug:` abort, and the `signum` doc bug. See that task.

## Closed 2026-09-10 (backlog prune)

Bookkeeping only — closed 2026-09-10. Successor `bug-arith-sexpr-intrinsics-zero` is the live, comprehensive task.
