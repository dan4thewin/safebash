---
title: "BUG: ${var:OP…} inside (( ))/$(( )) silently compiles to 0"
status: done
priority: P1
kind: bug
depends: []
fixed_by: "DD#77 Stage A (arith ${…} routed through parse_parameter_expansion)"
resolved: "2026-08-19 — done for the native/c gate (verified: no silent-0; $(( ${s:-9} )) → 5, pruned ops reject SB4034). The only residual is janet's arith VALUE path reading 0 for ${…}, tracked as janet-arith-subscript-read (P3, oracle backend — not the gate)."
gates:
  - "`s=5; echo $(( ${s:-9} ))` yields 5 (not 0/empty) — or the form is a clean reject, never silent-0"
  - "no `${var:OP…}` param-expansion in any arith context silently miscompiles"
---

# Symptom (silent miscompile — zero-tolerance) — FIXED

Fixed in DD#77 Stage A: `braced_body_token` (parse/arith.rs) now routes any
operator-bearing `${…}` body through the real `parse_parameter_expansion` →
`ArithExpr::Expansion`, so `${s:-N}`/`${s:?msg}`/`${s#…}`/etc are genuine
expansions instead of a bogus `DollarVar("var:OP…")` → 0. native+c
(arithmetic.sh pex.1-4); janet's arith VALUE path still reads 0 for an embedded
`${…}` (pre-existing janet-arith-subscript-read, now generalized — pex.1-3
guarded native,c). The DD#77 *disposition semantics* (strict trigger,
bare-string error) are separate follow-on stages.


Inside `(( ))`, `$(( ))`, and `if (( ))`, every `${var:OP…}` parameter
expansion that carries an *operator* silently evaluates to **0**:

    s=5;     echo $(( ${s:-9} ))      # → (empty/0), want 5
    s=hello; echo $(( ${s:-0} + 1 ))  # → 1, i.e. ${s:-0}→0 always
    s=5;     echo $(( ${s:+1} ))      # → 0
    s=hello; echo $(( ${s#he} + 0 ))  # → 0
    s=5;     if (( ${s:-9} > 3 ))…    # → false (reads 0)

Only three braced forms work in arith: `${var}` (plain), `${#var}` (length,
DD#75 §2), `${var[i]}` (subscript, arith-subscript-values). Bare `$(( s + 1 ))`
also works (atoi path).

# Root cause

`sash/src/parse/arith.rs` `braced_body_token` (≈562) classifies only the `#`
length form and the `[idx]` subscript form; **every other body falls through to
`ATok::DollarVar(body)`** — so `${s:-0}` becomes a "variable" literally named
`s:-0`, which is undefined → 0. The `:-`/`:?`/`#`/`%`/`/`/`:+`/`:=` operator is
swallowed into the bogus name.

# Fix direction (converges with a feature under discussion)

The correct fix is to route an arith `${…}` body through the **real
parameter-expansion parser** (as the BUG-1 subscript fix did for `${a[i]}`,
generalized), so `${s:-0}` becomes a genuine `DefaultValue` expansion, etc., and
is lowered via `expr_to_value` (`ArithExpr::Expansion`). At minimum an
unimplemented form must **reject**, never silent-0 (DD#72 "no silent arith
miscompiles").

NOTE: the *semantics* of `${s:-N}` / `${s:?msg}` in arith are entangled with a
DG design proposal — string→int conversion in `(( ))`/`$(( ))` should require an
explicit failure disposition (`${s:-N}` default / `${s:?msg}` abort), borrowing
DD#69's splat-disposition grammar. Do not prescribe the parse-then-atoi
semantics before that design is settled; the fix and the feature share this code
path. See the session discussion + (pending) DD.
