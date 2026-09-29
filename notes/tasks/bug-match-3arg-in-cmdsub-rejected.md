---
title: "3-argument `match S P ARR` inside `$( )` is SB4045, but DD#86 says it should be legal"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "`str s=$(match S P arr; …)` compiles"
  - "the 2-argument value form inside `$( )` is unaffected"
---

# What

DD#86: a `$( … )` body is an anonymous function body, so every statement legal
in a function body is legal there. The 3-argument `match S P ARR` is a
statement — it fills an out-param array, like `mapfile -m m` / `read -a a` /
DD#60's `argv N a`, all of which are now legal in a body.

It is SB4045 inside `$( )` and accepted everywhere else. That position
dependence is the tell.

`matchall` should be checked at the same time — it has the same shape.

# Where

- `sash/src/validate.rs` — whatever emits SB4045; check whether the guard is
  keyed on the syntactic position (`$( )`) rather than on the arity.

Filed 2026-09-29 while writing specs for journal 444.
