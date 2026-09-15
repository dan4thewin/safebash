# 324 — the operand changes kind when you move it

DD#31 is one sentence: *"Only postfix `x++` and `x--`. No prefix forms."*
`sash-convert` had never implemented it. Journal 323 found that out
sideways — the converter's prefix output used to compile, because
`lower_c_style_for` discarded parse errors, and fixing that bug turned a
silent converter gap into a visible one.

DG chose disposition **(a)**: rewrite where the rewrite is sound, annotate
where it is not.

## The line between the two

One question decides it: **does anything read the value?**

`++i` and `i++` differ only in what the expression evaluates to. They have
identical effects on `i`. So wherever the value is discarded, `i++` is not
an approximation of `++i` — it is exactly it, and the rewrite is silent and
complete. Wherever the value is read, there is no sound rewrite at all:
`$(( i++ ))` is not `$(( ++i ))`, and sash rejects mutation inside a read
context anyway, so there is nothing to rewrite *to*. That case gets a
`CHECK:` naming the hoist.

| context | CST shape | disposition |
|---|---|---|
| `(( ++j ))` | `compound_statement` whose `child(0)` is `((` | rewrite |
| `(( x++, ++k ))` | comma element — a direct child of the same node | rewrite |
| `for (( ++i; …; ++i ))` | `c_style_for_statement` fields `initializer` / `update` | rewrite |
| `for (( ; ++i < 3; ))` | field `condition` | `CHECK:` |
| `if (( ++i ))` / `while (( --n ))` | the `compound_statement` **is** the `condition:` field of its parent | `CHECK:` |
| `$(( ++i ))`, `(( y = ++i ))` | value position | `CHECK:` |

The `if`/`while` row is the one worth writing down. `(( ++i ))` as a
statement and `(( ++i ))` as an `if` condition are **the same node kind with
the same children**. Nothing about the node distinguishes them. The only
discriminator is which field of the *parent* the node occupies — `condition`
or nothing. A handler that looks only at itself and its children cannot tell
these apart, and would have rewritten a read.

## The bug inside the fix

The first cut handled every case above correctly and still did nothing at
all to a `for` header. The dispatch fired, the soundness test returned
`true`, and no patch appeared.

The operand lookup was `child_variable_name` — find the child whose kind is
`variable_name`. Inside `(( ++j ))` that is exactly what tree-sitter-bash
produces. Inside a `for` header it produces a **`word`**. Same source text,
same operator, same grammar; the operand's node kind depends on which
enclosing construct it sits in. `child_variable_name` returned `None` and
the function returned early — no patch, no `CHECK:`, no diagnostic. Silence.

The replacement accepts either kind, and requires the `word` to be
identifier-shaped so `++a[i]` (a `subscript`) does not get mistaken for a
scalar. That case now emits a `MANUAL:` line rather than vanishing.

The general lesson is the one that keeps recurring: **an early `return` on a
`None` is a decision not to handle something, and it looks exactly like
having handled it.** The only reason this surfaced at all is that the
end-to-end check was "does the output differ", not "did the function run".

## Verified

`for (( i=0; i < 3; ++i ))` converts, compiles, and runs byte-identical to
bash. Ten shapes checked by hand, eight pinned as specs `57.156`–`57.163`.

Sweep **4861 passing, 10 skipped, 0 failing**. 27 `sash-convert` tests green.
