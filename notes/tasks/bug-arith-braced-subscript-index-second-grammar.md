---
title: "BUG: inside `$(( ))`, an operator-free `${a[K]}` parses K with a second, smaller index grammar, so legal indexes are SB4041"
status: open
priority: P2
kind: bug
depends: [bug-subscript-key-check-skips-positions]
gates:
  - "inside `$(( ))`, the index of `${a[K]}` is parsed by `parse_subscript_index`, the grammar every other position uses; `subscript_index_expr` and `dynamic_subscript_var` are deleted if nothing else calls them"
  - "every row of the first What table prints its expected value in both backends"
  - "every row of the second table draws its expected diagnostic, and none compiles"
  - "ig11 and ig12 stay SB4041, and the rows that work today are unchanged"
---

## What

DD#80's 2026-10-03 amendment ("one grammar, three spellings") made its index
grammar "the *only* subscript grammar in the compiler". One more remains.

Inside `$(( ))`, the index of an operator-free `${a[K]}` is parsed by a
grammar that recognises only `$name` and `${name}`. Every other index becomes
a literal of its source text, which SB4041 then rejects as ambiguous. The
same index parses correctly outside arithmetic, and inside it when the
expansion has an operator.

Results were measured after F15. They are the same in both backends, on the
base commit 1309385 and on the batch-531 branch.

### Legal indexes that are rejected only inside `$(( ))`

| Id | Program | Today | Expected | Outside `$(( ))` |
|---|---|---|---|---|
| ig1 | `int array a=(10 20)` / `int i=0` / `echo $(( ${a[$(( i + 1 ))]} + 1 ))` | SB4041 at 3:6 | `21` | ig8: `echo ${a[$(( i + 1 ))]}` prints `20` |
| ig6 | `int map m=([k]=5)` / `echo $(( ${m["k"]} + 1 ))` | SB4041 at 2:6 | `6` | ig9: `echo ${m["k"]}` prints `5` |
| ig10 | `int array a=(10 20)` / `echo $(( ${a[ 1 ]} + 1 ))` | SB4041 at 2:6 | `21` | ig14: `echo ${a[ 1 ]}` prints `20` |
| ig13 | `int map m=([k]=5)` / `echo $(( ${m[$(echo k)]} + 1 ))` | SB4041 at 2:6 | `6` | ig15: `echo ${m[$(echo k)]}` prints `5` |

ig6's diagnostic contradicts itself: "ambiguous index `["k"]` — a subscript
takes a direct key, not a computed one. Quote a literal key (`["…"]`), use a
variable (`[$name]`), or open arithmetic explicitly (`[$(( … ))]`)". It
rejects the first form it recommends. The third form is ig1, which is
rejected too.

### The wrong diagnostic

| Id | Program | Today | Expected |
|---|---|---|---|
| ig5 | `int array a=(10 20)` / `echo $(( ${a[$(echo 1)]} + 1 ))` | SB4041 | SB4030, capture as an index, as `echo ${a[$(echo 1)]}` draws |
| ic2 | `array a=(10 20)` / `echo $(( ${a[$(echo 1)]} ))` | SB4041 | the same |
| a12 | `array a=(10 20)` / `str k=x` / `echo $(( ${a[${k:s/x/1/}]} ))` | SB4041 | the same, worded by `bug-sed-index-capture-advice` |
| ic3 | `map m=([1]=20)` / `echo $(( ${m[$(echo 1)]} ))` | SB4041 | SB4030, string cell in arithmetic, as ic10 (`map m=([1]=20)` / `str k=1` / `echo $(( ${m[$k]} ))`) draws |
| m09 | `map m=([1]=20)` / `str k=x` / `echo $(( ${m[${k:s/x/1/}]} ))` | SB4041 | the same as ic3 |

### Unchanged

Inside `$(( ))`, these work today and must keep working:

- ig3 `${a[1]}`;
- ig4 `${a[$i]}`, with `int i=1`;
- ig7 `${m[k]}`;
- ig16 `${m[$k]}`, with `str k=k`.

ig11, `$(( ${a[i+1]} + 1 ))`, is SB4041 at 3:6, and so is its outside twin
ig12. That is DD#80's rule, not this bug: a subscript takes a direct key.

## Cause

In parse/arith.rs, `braced_body_token` sends a `${…}` body that ends in `]`
to `try_braced_subscript`, which yields `ATok::Subscript(name, index_text)`.
`parse_expansion_atom` builds that index with
`parse::expr::subscript_index_expr`, its only caller.

That function recognises `$name` and `${name}` (through
`dynamic_subscript_var`). It makes everything else an `ExprKind::Literal` of
the source text, which validate's `ambiguous_index_diag` rejects as SB4041.
Everywhere else, an index is parsed by `parse_subscript_index`, the grammar
DD#80 describes.

A body with an operator takes another route: it goes to `ATok::ParamExpansion`
and the real parameter-expansion parser (DD#77). That is why
`${a[$(echo 1)]:-0}` inside `$(( ))` gets a correctly parsed index, and then
meets `bug-subscript-key-check-skips-positions` instead.

## Fix

There are two ways to give this index the real grammar:

- parse it with `parse_subscript_index`;
- or route every `${name[…]}` body through `ATok::ParamExpansion`, as the
  operator forms already are.

Either way, delete `subscript_index_expr` and `dynamic_subscript_var` if
nothing else uses them.

This must land after `bug-subscript-key-check-skips-positions`, hence the
`depends`. Today SB4041 is the only thing stopping ig5 and ic2. Once they
parse, nothing checks their key inside `$(( ))` until that task lands. They
would compile, just as `$(( ${a[foo]} + 1 ))` already does (that task's ih1).

Found during the batch-531 port (step F15), while probing the sed-index
advice. Filed under DG's rule for the port that questions and uncovered cases
become backlog tasks.
