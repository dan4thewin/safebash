---
status: open
priority: P3
depends: dd86-sb5049-scope
gates: DG — the SB5049 scope question
---

# `is_bare_redeclare_noop` is unreachable under current SB5049/SB5050

Found in the branch-coverage ranking: `transform/statements.rs:641`, the call
guarding `lower_declaration`'s early return, is 0 hits against 3,503.

```rust
/// `declare var` (no value, not -a/-A) on an already-defined variable
/// is a no-op — don't clobber the existing value.
fn is_bare_redeclare_noop(a, is_array, is_assoc, state) -> bool {
    let is_bare =
        matches!(&a.value.kind, ExprKind::Literal(s) if s.is_empty())
            && !is_array && !is_assoc;
    is_bare && state.lookup(&a.name).version > 0
}
```

It is a leftover from when sash accepted bash's `declare x`. Every route to it
is now closed by an earlier diagnostic — verified against the live binary:

| input | result |
|-------|--------|
| `declare x` | SB5011 — `declare` is not a keyword, just an unknown command |
| `str x = "a"` then `str x` | SB5049 — one declaration per name per scope (DD#86) |
| `str x` alone | SB5050 — assigned but never read |

A bare declaration needs an empty-literal value *and* a prior version of the
same name. SB5049 forbids the second declaration, and a bare declaration that
is not a redeclaration is a dead variable. There is no surviving input.

## Why it was not removed

**This is coupled to the open SB5049 scope question.** If SB5049 is narrowed
so that redeclaration is legal in some scope, `str x = "a"; str x; echo $x`
becomes reachable and this no-op is exactly what stops the second declaration
from clobbering the first. Deleting it now would quietly remove the machinery
that a relaxed SB5049 would depend on.

## Decision needed

- **SB5049 stays as-is (all redeclaration rejected)** → the function is dead;
  remove it and its call site.
- **SB5049 is narrowed** → the function becomes live; it needs a spec proving
  the no-op semantics (second bare declaration does not clobber).

Either way it should not stay in its current state: unreachable, untested, and
documented in terms of a `declare` keyword the language no longer has.
