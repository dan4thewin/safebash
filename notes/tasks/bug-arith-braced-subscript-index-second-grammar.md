title: "BUG: inside `$(( ))`, an operator-free `${a[K]}` parses K with a second, smaller index grammar, so legal indexes are SB4041"
status: done
priority: P2
kind: bug
depends: [bug-subscript-key-check-skips-positions]
gates:
  - "inside `$(( ))`, dollar/braced cell imports use the ordinary parameter-expansion route and shared `parse_subscript_index`; the second token/parser route and both narrower helpers are deleted"
  - "every row of the first What table prints its expected value in both backends"
  - "every row of the second table draws its expected diagnostic, and none compiles"
  - "under the accepted operand-word grammar, ig11 and ig12 reject string array indices with SB4030 rather than the retired SB4041 shape rule; existing legal imports remain legal"

## Resolution, 2026-11-25

Implemented on `err-attempt-trial` together with the prerequisite key-type
repair. Operator-free cells now use `ATok::ParamExpansion`, just like cells
with dispositions. Dollar imports take the same route. `ATok::Subscript`,
`try_braced_subscript`, `subscript_index_expr` and `dynamic_subscript_var`
are deleted; raw quoted token text is preserved rather than reconstructed
through a smaller grammar.

DG also selected ordinary operand-word keys for direct reads, assignments
and unset. Thus `${a[i+1]}` now parses the literal string key `i+1` and
receives SB4030 for an array. This deliberately supersedes the old
ig11/ig12 SB4041 requirement below. Bare arithmetic `a[i+1]` still means
arithmetic indexing; bare `m["key"]` admits a wholly quoted string map key.
Dollar/braced imports remain values, never mutation locations.

`maparith.1`-`.3` exercise the unified imports; `wordkey` and `keytype`
pin word grammar and key typing. Disabling only bare quoted-map parsing
leaves the import controls green while making 15 other checks fail per
backend. The original compiler's changed-suite controls and the ten
independent reversions are recorded in
[journal 670](../journal/670-operand-word-collection-keys.md).
The final 13086-check gate passes with 13 existing skips.

## What (historical)

The following measurements and "today" wording describe the original
filing, before the operand-word ruling and this implementation.

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

At filing, ig11, `$(( ${a[i+1]} + 1 ))`, was SB4041 at 3:6, as was its
outside twin ig12. That was DD80's then-current direct-key shape rule,
not this bug. The accepted operand-word grammar now makes both SB4030
string-array-index refusals instead.

## Cause (historical)

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

## Delivered fix

The normal parameter-expansion route is shared, rather than maintaining
another arithmetic-only cell parser. `parse_subscript_index` owns the
operand-word grammar in either context. Only bare arithmetic locations
have the arithmetic/wholly-quoted distinction.

The prerequisite scoped key pass is delivered in the same change.
Historically, SB4041 was the only thing stopping ig5 and ic2; removing
the smaller grammar without that pass would have exposed the same
unchecked array keys already demonstrated by the prerequisite's ih1.

Found during the batch-531 port (step F15), while probing the sed-index
advice. Filed under DG's rule for the port that questions and uncovered cases
become backlog tasks.
