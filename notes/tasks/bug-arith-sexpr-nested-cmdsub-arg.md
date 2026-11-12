title: "bug: an arith-sexpr call arg that is a nested $(( )) silently lowers to 0"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`int x=$(( (dbl $(( 2 + 1 ))) ))` computes dbl(3)=6, not 0 (nested $(( )) arg to a DD#58 sexpr call)"
  - "a plain arg `(dbl 3)` and a bare-var arg `(dbl m)` already work — only the nested $(( )) arg is wrong"
  - "0 native/c/janet spec regressions"

# bug: nested `$(( ))` arg to an arith-sexpr call → silent 0

## Symptom

A DD#58 arithmetic sexpr call `(f ARG)` inside `$(( ))` mis-lowers when ARG is
itself a nested `$(( ))` expansion — the callee receives a wrong/garbage arg and
the result is a **silent 0** (worst class per the bug policy):

```sash
fn dbl -> int { return $(( $1 * 2 )); }
int x=$(( (dbl $(( 2 + 1 ))) ))   # x = 0   (want 6)
int y=$(( (dbl 3) ))              # y = 6   (correct — plain literal arg)
int z=$(( (dbl m) ))              # correct — bare-var arg (arith reads m)
```

The emitted args table for the nested-`$(( ))` form gets an extra spurious
element whose value references an out-of-scope temp (`p_ptr`), so the callee's
`$1` reads garbage.

A parenthesized arith arg `(dbl (2 + 1))` is instead a **compile error** (exit
1) — at least not silent, but also unsupported.

## Pre-existing (NOT the args model / tail recursion)

Discovered 2026-08-21 while testing tail recursion (`return $(( (f $(( $1 - 1 )))
))`). Confirmed pre-existing and independent of positionals: the non-recursive
`(dbl $(( 2 + 1 )))` reproduces it. The DD#75 §5 args model and the tail-call
loop both handle well-formed args correctly (`(f n1)` with `int n1=$(( … ))`
works); this is purely about how `funccall_to_ssa` lowers an arg that is a
nested arith expansion.

## Fix direction

In `transform/expr_handlers.rs::funccall_to_ssa`, the sexpr args are lowered via
`arith_to_ssa(a, …)`. When `a` is an `ArithExpr::Expansion` wrapping a `$(( ))`
(or the parser produced an extra arg), the arg count / lowering is wrong.
Investigate how `(f $(( … )))` parses in `parse/arith.rs` (the DD#58 sexpr arg
list) — likely the nested `$(( ))` is split into multiple sexpr args, or the
Expansion lowers to a 2-tuple that the IndexedArray/argv builder mis-sizes.
Idiomatic tail-recursion accumulators sidestep it by binding a local first
(`int n1=$(( n - 1 )); return $(( (f n1) ))`), so this is not blocking, but the
silent-0 must be fixed (reject or compute correctly).
