---
title: "compound-condition catch — if a && b, if a; b, if-body"
date: 2026-08-19
---

# Compound-condition catch + list/body abort (DD#55)

DG resolved the three-part `compound-condition-abort-catch` decision:

1. **`&&`/`||` operands in a condition are CAUGHT** (bug fix). `if a && b`
   aborted on `b`'s failure instead of taking the else. The whole `&&`/`||`
   aggregate is the condition, so every operand's failure is caught.
2. **`if a; b` is `a; if b`** — the non-final `a` is a bare command whose
   status is discarded → it aborts. Only `&&`/`||` operands are caught; a `;`
   list is not.
3. **A failing if-body final command aborts** — its status is discarded
   (bare-command-uncaught), like a top-level `false`.

## The bug (part 1)

`mark_unchecked_in_conditions` unchecks the last status-bearing inst of any
block that terminates in a `Branch`/`Loop`. For `a && b`, the entry block
runs `a` and ends in the short-circuit `Branch` → `a` unchecked (caught).
But `b` sits in the `and.right` block that ends in `Goto(merge)` → never
unchecked → aborts. So `if true && false` aborted instead of taking else.

The insight: `state.in_condition > 0` already precisely marks a List in a
**caught position** — it's bumped by `lower_if`/`lower_while` around the
condition, *and* by `lower_list_stmt` around its own left operand (so the
left of an enclosing `&&`/`||` is caught too). Fix: when `caught`, uncheck
the right operand's terminal command. The left is already handled by its
`Branch` block; at top level the rightmost operand stays checked and aborts
(its status is discarded).

Parts 2 and 3 needed no code change — a `;` list's non-final member and an
if-body command are ordinary bare commands (checked=true, block ends in
`Goto`), so they already abort. Only the specs (bc.a7/e1/e2) needed
rewriting to the ratified behavior.

## Coupled fix: the `||` $?-reset

The `||` arm reset `$?=0` after the right side ("if the right completes, the
list succeeded") — masked by the abort before, now exposed: `if false ||
false` printed `y`. That reset is correct for a **top-level guard** (`cmd ||
fallback` shouldn't exit with the guard's failure) but wrong in a
**condition**, where the real status is the condition value. Now skipped when
`caught`. Full truth table verified (native+c): `false || false` → else,
`false || true` → then, top-level `false || { … }` still catches.

## Backends

native + c correct. **janet** emits conditions from the AST (not the SSA
transform), so it bypasses this fix and still mis-evaluates `if a && b` —
a pre-existing control-flow gap (why brace.sh is native-only). Noted in
`cf-backend-gaps-brace-surfaced`; errors.sh cc.1-4 guarded `native,c`.

## Verification

spec brace.sh bc.a6b/a7/a8/a9/e1/e2 (native) + errors.sh cc.1-4 (native,c);
also corrected errors.sh's pre-existing stale plan (64→79) and un-skipped
the four deferred brace tests. 3-mode sweep: 0 regressions, +14 passing.
`lower_list_stmt` cc7/cog7 (under gate). DD#55 addendum.
