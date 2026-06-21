---
title: "BUG — `$#` (and likely other specials) read as empty/0 inside (( ))"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "$# inside (( )) evaluates to the argument count (e.g. `(( $# < 2 ))` / `$(( $# ))`), matching how $# works elsewhere"
  - "audit other specials in arith: $?, $$, $!, positionals $1.. — each reads its value inside (( ))"
  - "re-enable the #[ignore]d numeric_with_argc codegen_test"
---

# BUG — `$#` reads empty inside `(( ))`

## Symptom

`echo $(( $# ))` prints empty for any argv; `(( $# < 2 ))` is always
true (reads `$#` as 0/empty). `$#` works in other contexts.

## Likely cause

`$#` inside `(( ))`/`$(( ))` is collected by
`arith.rs::collect_arith_content` from the bash scanner and re-parsed by
the arith byte-lexer. `$#` (and other specials: `$?`, `$$`, `$!`,
positionals) likely isn't carried through the collect/`tok_to_text` step
or isn't recognized by the arith lexer/transform as a readable operand,
so it lowers to empty → 0. Investigate `tok_to_text` / `scan_arith_ident`
handling of `$#`-style tokens and the SSA lowering of the resulting
operand.

Note interaction with DD#46 (top-level `$#`/positionals are
function-local; top-level use is an SB error). The codegen test path
bypasses validation, so the *codegen* read must still be correct where it
is reachable (inside functions, and for the `$(argc)`-style program-arg
model). Decide whether arith should read `$#` directly or whether sash
directs to `$(argc)` — coordinate with DD#46/argv-trio.

## Discovery

Surfaced 2026-06-20 converting `numeric_with_argc` (`[[ $# -lt 2 ]]` →
`(( $# < 2 ))`) after the for-loop-arith-condition work. Pre-existing.
