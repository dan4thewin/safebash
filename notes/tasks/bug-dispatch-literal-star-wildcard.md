---
title: "BUG — `[[ ]]` dispatch treats literal `*` as a catch-all wildcard (`||` is the only default)"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`x=hi; [[ $x == * { echo star } || { echo default } ]]` prints `default` (literal `*` != `hi`), not `star`"
  - "`x='*'; [[ $x == * { echo star } || { echo default } ]]` prints `star` (literal `*` == `*`)"
  - "the `|| { }` default still runs when no arm matches (its representation no longer relies on a `*` pattern)"
  - "native + c + janet identical; 0 spec regressions"
---

# BUG — dispatch `*` is a wildcard, but DD#66 dispatch has no wildcards

Found 2026-07-04 while designing multi-exact dispatch arms (DG caught it).

## Repro

```
$ sash -c 'x=hi; [[ $x == * { echo star } || { echo default } ]]' -o /tmp/b; /tmp/b
star          # WRONG — `*` matched `hi`. Expected: default
```

Under DD#66 the dispatch form has **no pattern language and no wildcards** —
`==` is strict exact equality and the **only** default is `|| { }`. A bare
`*` pattern must therefore be the *literal string* `*` (exact-match), not a
catch-all.

## Root cause

The `|| { }` default arm is synthesized in `parse_dispatch`
(`sash/src/parse/parser.rs`) as a `CaseItem` whose pattern is
`ExprKind::Literal("*")`. Lowering then treats *any* arm carrying a `*`
pattern as unconditional, via `is_wildcard_arm`
(`sash/src/transform/control_flow.rs`):

```rust
fn is_wildcard_arm(item: &CaseItem) -> bool {
    item.patterns.iter().any(|p| matches!(&p.kind, ExprKind::Literal(s) if s == "*"))
}
```

This is a `case`-era leak (case's `*)` really was a catch-all). Now that
`case` is gone, the synthetic default `*` and a user-written literal `*`
are indistinguishable, so `== *` wrongly matches everything.

## Fix direction (representation choice — for DG)

Give the `||` default its own representation instead of overloading `*`:
- **(preferred) `is_default: bool` on `CaseItem`** — `parse_dispatch` sets
  it on the `||` arm; lowering makes an `is_default` arm the unconditional
  goto and drops `is_wildcard_arm` entirely, so `*` lowers as an ordinary
  literal exact-match pattern.
- (alt) a non-forgeable sentinel arm. Less clear than a flag.

Affected: `parse_dispatch` (default-arm construction), `ir::CaseItem`,
`control_flow.rs` (`is_wildcard_arm` / `emit_dispatch_arm`), and the janet/c
emitters' wildcard/`_`-arm detection (`emit_janet.rs` `p == "\"*\""`,
`emit_c` dispatch). Add spec/dispatch.sh cases for literal-`*` match + a
`||`-default-still-fires regression.

## Relationship

Surfaced alongside `bug-regex-rhs-space-quote`. Part of the DD#71 cluster
(`notes/decisions/DD71-operand-words-value-bindings.md`): `*` is an ordinary
literal operand (dispatch has no pattern language / no wildcards — `||` is
the only default), the same "no pattern language" spirit as DD#66's `==`.
Land together with `operand-word-absorb-mode` so dispatch pattern semantics
are consistent (literal `*`, multi-exact arms, operand-word lexing).
