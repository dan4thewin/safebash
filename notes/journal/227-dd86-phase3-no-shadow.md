# 227 — DD#86 Phase 3: no shadowing (SB5049)

DD#86 forbids shadowing: a name is a global *xor* a local, never a local
hiding a same-named global. Phase 3 enforces it.

## The rule

- A name **declared at top level** (a typed `str`/`int`/`array`/`map`/
  `const` declaration, or a bare `x = v`) is a **global**.
- A **typed re-declaration** of that name **inside a function** →
  **SB5049** (redeclaration error). There is no way to create a local
  that shadows a global.
- A **bare assignment** to a global inside a function is *not* flagged —
  it writes the global (frictionless, DD#86). Only typed declarations
  shadow.
- A typed declaration of a **new** (non-global) name in a function is a
  fresh function-local — fine.

## Implementation

`check_no_shadow` in `validate.rs` (a new pass, after
`check_const_reassignment`):

1. `collect_top_level_decls` walks the top-level statements (and the
   `__END__` block) gathering every declared/assigned name **without
   descending into function bodies** — those are the globals.
2. `scan_fn_shadow` finds each function body; `scan_body_shadow`
   recursively scans it (including nested functions and control-flow
   bodies) and flags every `StmtKind::Declaration` whose name is in the
   global set.

Only `StmtKind::Declaration` (a *typed* declaration) is flagged; a bare
`StmtKind::Assignment` writes the global. The diagnostic points the user
at the two fixes: write the global with a bare assignment, or rename the
local.

## Verification

`spec/scope.sh` 86.7–86.12 (native,c,janet): `int`/`str`/`const`/`array`
re-declaration of a global inside a fn → SB5049; a bare global write and
a fresh fn-local are *not* flagged. Full 3-mode sweep vs HEAD `dcf2c97`:
**native 0 regressions +6, c 0 regressions +6**, janet +8 (the six plus
Phase 2's 18.1/18.2, which janet now also passes). No existing spec
shadowed a global, so nothing needed rewriting.

Note: 86.10/86.11 interpolate an int into a `"prefix=$n"` string (not a
bare `"$n"`) because the janet backend still can't pass a bare int var to
an external command (`os/execute` wants a string) — a pre-existing janet
coercion gap, orthogonal to this phase.

## Next

Phase 4 (dead-variable rejection — assigned but never read → error, the
dual of DD#62 SB5031), then Phase 5 (converter + GUIDE/SUBSET + DD#35).
