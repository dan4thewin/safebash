---
title: "BUG — `$#` (and likely other specials) read as empty/0 inside (( ))"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "specials in (( )) read through the normal var path: $?/$$/$! clean; positionals function-local + top-level SB error (DD#46) — same as outside arith"
  - "audit other specials in arith: $?, $$, $!, positionals $1.. — each reads its value inside (( ))"
  - "re-enable the #[ignore]d numeric_with_argc codegen_test"
---

# BUG — `$#` reads empty inside `(( ))`

## Resolution (2026-06-21) — option A, done

Fixed across native + c (the reference backends) and the janet leaf:

- **Lexer** (`parse/arith.rs::scan_arith_dollar`): now scans the special
  single-char names `# ? $ ! @ *` (digits `$1..$9` already scanned), so
  `$#`/`$?`/… produce a real `DollarVar(name)` instead of an empty name +
  stray sigil. Lowers through the normal var path (`expr_handlers.rs`:
  `DollarVar(name) → SsaValue::Var(lookup(name))`).
- **Validator** (`validate.rs::check_positionals`): now descends into
  `ExprKind::Arithmetic` and bare `StmtKind::ArithTest`, so DD#46 is
  enforced inside `(( ))`/`$(( ))` — top-level `$#`/`$@`/`$1..` →
  SB5024; `$?`/`$$`/`$!` stay clean everywhere.
- **janet** (`emit_janet.rs`): arith `DollarVar` now lowers `$#` →
  `(length _args)`, `$?` → `__last_exit`, `$N` →
  `(scan-number (get _args …))`; the positional scan
  (`_arith_uses_args`) descends into arith so a function using `$#` in
  `(( ))` gets `[& _args]`. Also fixed a pre-existing janet double-paren
  bug in arith-value emission (`(string ((+ …)))` → `(string (+ …))`)
  that crashed *all* `x=$(( … ))` — repaired 6 pre-existing janet specs.
- `numeric_with_argc` codegen_test reframed inside a function and
  un-ignored. New specs argv.sh av.h1–av.h4 (av.h3 guarded native,c per
  bug-janet-arith-condition; janet overflow exposed → bug-janet-int-overflow).

Verified: native 2-fail unchanged (+4 pass), c 61-fail unchanged
(+4 pass), janet −6 fail / +9 pass; rust codegen/janet suites: zero new
failures.

## Decision (DG, 2026-06-21): option A

Read specials through the normal var path so DD#46 governs them: fix the
arith lexer (`scan_arith_dollar`) to scan `$#`/`$?`/`$1`/`$@`/`$*`/`$$`
etc., and lower them exactly as outside `(( ))`. Consequences: `$?`/`$$`/
`$!`/`$LINENO` are clean and work; positionals (`$1..$N`, `$@`, `$*`,
`$#`) work inside a function and are an SB error at top level (DD#46).
Reframe the `numeric_with_argc` codegen_test accordingly (move inside a
function or use `$(argc)`) — top-level `$#` is *not* meant to work.

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
