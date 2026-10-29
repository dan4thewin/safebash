---
title: "bug: SB5012 is blind to arithmetic status reads"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`echo $(( $? ))` with no preceding command is SB5012"
  - "`echo $(( (status) ))` likewise — both spellings, not just the sugar"
  - "`$(( a[(status)] ))` likewise: a subscript index is not a blind spot"
  - "one `$(( … ))` holding several reads reports one diagnostic, not one per read"
  - "a preceding command still makes the arithmetic read legal"
---

## What

SB5012 rejects a status read with no preceding command to have set it. It did
so for the word spellings and not for the arithmetic ones:

```sh
echo "$?"             # SB5012  ✓
echo "$(status)"      # SB5012  ✓
echo $(( $? ))        # accepted, yields 0   ✗
echo $(( (status) ))  # accepted, yields 0   ✗
int n=$(( (status) )) # accepted             ✗
```

Pre-existing at HEAD — confirmed by `git stash` + rebuild, so not fallout from
the DD#111 desugar that landed alongside it.

## Why

`find_status_reads` walks via `walk_exprs_in_stmt`, which visits `Expr` nodes.
An arithmetic `(status)` is an `ArithExpr::FuncCall`. That is a **different
type**, so no `Expr` walk can reach it however thorough. `$?` in a word and
`$?` in arithmetic are one read to a user and two unrelated types to the
compiler, and only one of the two was being checked.

`assert_no_magic_sugar` — the DD#111 invariant guard — had the identical blind
spot one layer up, and its doc comment had **predicted this exact
consequence**: "the checks that look for the intrinsic form — SB5012's `$?`
state machine above all — would simply stop seeing reads there."

### The walker had a wildcard

`walk_arith_intrinsic_calls` already existed and looked like the obvious reuse,
but it spelled out its own recursion behind a `_ => {}`. The wildcard swallowed
`ArithExpr::Subscript` and `ArithExpr::SubscriptAssign`, both of which carry
`ArithExpr` children, so `$(( a[(status)] ))` stayed blind even after the
`Expr`-level half was fixed. Its one other caller is the sexpr **arity** check,
blind under a subscript index for exactly as long.

## Fix

- `walk_arith_intrinsic_calls` delegates descent to `ArithExpr::children()`,
  which is already total over the enum. Four lines, no wildcard, and the two
  dropped variants become unrepresentable rather than merely handled.
  (2026-10-29: since replaced by the shared `ir::walk_arith_funccalls`,
  journal 544.)
- `expr_reads_status` (new) recognises both node types; `find_status_reads`
  calls it. The arithmetic tree has no per-node span, so the enclosing
  `$(( … ))` carries the diagnostic, once per expansion.

## Coverage

`spec/arithmetic.sh` `as.1`–`as.8` (`plan=338` → `346`). `as.4` is the
subscript regression guard; `as.5` pins one-diagnostic-per-expansion; `as.6`–
`as.8` pin the unchanged half.

None of the five pre-existing specs reading status inside arithmetic
(`argv.sh:244`, `arithmetic.sh` 31.22/31.23/sx.5, `convert.sh:944`) needed
touching — every one has a preceding command.

Journal 455.
