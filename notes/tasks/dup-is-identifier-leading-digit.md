title: "Two `is_identifier` copies disagree, and one contradicts its own doc"
status: done
priority: P3

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

# DONE 2026-09-12

**Reachable, and it produced working binaries for input bash rejects.** The
filing was right to say "do not assume it is inert."

## Step 1 — measured before changing anything

Three callers of the broken predicate, all reachable with a leading-digit word:

| caller | probe | before |
|---|---|---|
| `looks_like_assignment` | `9x=1` / `0=1` / `123=5` | parsed as an assignment; reached SB5050 "dead variable", i.e. the NAME was accepted |
| `looks_like_dynamic_subscript_assign` | `9x[$i]=1` | parsed as a subscript assignment; reached SB5068 |
| `looks_like_regex_mutation` | `9x =~ s/a/b/` | **compiled cleanly to a static binary** |

`9x=1` followed by `echo "${9x}"` also compiled. Bash answers
`9x: command not found` to all of them — `9x` is not a valid name there.

So the hole was not cosmetic: sash accepted a class of variable names bash
does not, and the `=~` path emitted a binary for it.

## Step 2 — consolidated

`parser.rs`'s copy is deleted; the file now imports
`parse::expr::is_identifier`, which was already `pub(crate)` from the DD#100
Part D work. One definition, as the filing asked.

## Step 3 — `is_identifier_or_subscript`

Was a flat character-set test (alphanumerics plus `_ [ ]`), so it also accepted
`][`, `a]b[`, and `[[[` as assignment targets. Replaced with a structural
check: `is_identifier` on the part before `[`, plus a single well-formed
non-empty `[…]` with no nested brackets. `aw.20` pins `a]b[=1`.

## The resulting error is uniform, by DG's rule

A word that is not a valid identifier is no longer an assignment, so it falls
through to command position and errors there — the same treatment DG chose for
the removed-builtin table ("error just as for any other unknown token in cmd
position"). No new diagnostic was invented.

The message it lands on is imprecise ("a variable or expansion can't be
resolved" for a word containing neither), but that is **pre-existing**: `a-b=1`
produced the identical text before this change. Filed separately as
`sb5011-misdiagnoses-malformed-assignment`; the specs here assert the code and
deliberately not the wording.

## Specs

`spec/assign_words.sh` `aw.11`–`aw.20` (plan 10 -> 20), all three modes.
Six negatives (`9x=1`, `0=1`, `123=5`, `9x[$i]=1`, `9x =~ s/a/b/`, `a]b[=1`)
and four positive guards (`x9=1`, `_x=1`, both subscript-assignment spellings,
a real `=~` mutation). Fail-first: all six negatives fail on the pre-change
compiler, all four guards pass on both — which is what a guard should do.

Sweep: 5351 passing / 41 failing / 567 skipped, zero non-janet failures, no
flips outside the 30 new rows.
