---
status: next
priority: P3
---

# Two `is_identifier` copies disagree, and one contradicts its own doc

`sash/src/parse/` carries two private `is_identifier` helpers with
different semantics:

`sash/src/parse/expr.rs:362` — correct:

```rust
/// A plausible `[A-Za-z_][A-Za-z0-9_]*` identifier.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
```

`sash/src/parse/parser.rs:2547` — **does not match its own doc comment**:

```rust
/// A word that is a valid identifier: [a-zA-Z_][a-zA-Z0-9_]*
fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
```

It never checks the first character, so `9x`, `0`, and `123` all pass as
identifiers. The neighbouring `is_identifier_or_subscript` (2552) has the
same hole.

## Why this matters

This is the recurring bug class: a predicate that answers a plausible
`true` where it should answer `false`. Every caller that uses it to decide
"is this word a variable name?" will treat a numeric word as a name.
Whether that is presently reachable is exactly what needs measuring — do
not assume it is inert.

## Work

1. Enumerate the callers of `parser.rs::is_identifier` and
   `is_identifier_or_subscript`, and for each determine whether a
   leading-digit word can reach it. Write a spec for any that can
   **before** changing the predicate, so the fix is fail-first.
2. Consolidate to one definition. `parse::expr::is_identifier` was already
   promoted to `pub(crate)` (see the DD#100 Part D commit) because
   `emit_janet.rs::janet_collection_operand` needed it; make `parser.rs`
   use that one and delete its copy.
3. Decide what `is_identifier_or_subscript` should be — probably
   `is_identifier(name_part)` plus a subscript check, rather than a
   character-set test that accepts `][`, `a]b[`, and `[[[`.

## Found

While fixing the Janet `join` operand gap during DD#75/DD#100 Part D
(b8). A third copy of the predicate was about to be written for
`emit_janet.rs`; promoting the correct one instead surfaced the
divergence.
