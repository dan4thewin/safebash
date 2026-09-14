---
title: 319 — Spelling the coercion the source never had
---

DG: *use abort with message disposition for converter.*

That settles `converter-string-var-arithmetic`, open since the braced-
subscript work. The problem, restated:

```sh
i=1
echo $(( i + 1 ))
```

Bash has one scalar type, so `i=1` is a string that arithmetic silently
coerces. Sash requires the coercion to be spelled, because "what if it
isn't a number" has two defensible answers and the compiler will not
pick one for you: SB4030 demands `${i:-N}` or `${i:?msg}`.

The converter answered neither. It emitted the bash spelling verbatim,
which does not compile. And this is not an exotic case — it is *most
bash scripts*: assign a numeric literal without `declare -i`, then do
arithmetic on it.

## The choice

Four options were on the table (a: silent `${i:-0}`, b: abort
`${i:?msg}`, c: `# MANUAL:` marker, d: infer `int i=1` by whole-script
dataflow). My recommendation was (d) with a (c) fallback. DG chose
**(b)**.

The case for (b) over (d), reading it back: (d) is the *faithful* option
only where it applies, and where it does not apply it degrades to (c),
which produces a script that does not compile. (b) always produces a
script that compiles and runs, and it is honest about what it did — the
message is in the output, in the source, where a human reading the
converted file will see it. (d)'s inference is invisible in exactly the
way that makes a migration tool hard to trust.

The cost is real and was documented before the decision: bash treats a
non-numeric string as 0, so the converted program **diverges** from the
original on exactly the inputs anyone cares about.

```
$ cat in.sh
i=abc
echo $(( i + 1 ))

$ bash in.sh          $ ./converted
1                     converted: i is not an integer   (rc=1)
```

Byte-identical-to-bash is the project's standard, and this breaks it on
purpose. The reading is that a script computing `1` from `abc` was
already wrong; the converter is not preserving a behavior, it is
surfacing a latent bug. That is a migration tool's job.

## Implementation

Three spellings of one read — `i`, `$i`, `${i}` — so three arms in
`walk_node`, all routed through `arith_disposition`, which declines when:

- the name is `declare -i` (converts to sash `int`, already well-typed);
- the name is a known env var (`convert_simple_expansion` already
  rewrites it to a `$(getenv …)` + attest pair, and two patches over one
  span collide);
- `in_arithmetic` says no;
- the node is **written**, not read.

That last one needed care. tree-sitter models `c = a + 1` as a
`binary_expression` whose operator happens to be an assignment, so the
only tell is the operator text — and `==`, `!=`, `<=`, `>=` all end in
`=` without writing anything. The set is enumerated rather than tested.
`k++` is a `postfix_expression`, `--k` a `unary_expression`, and a
c-style-for initialiser is a `variable_assignment`.

`${a[i+1]}` needed a second mechanism. bash parses a subscript index as
one opaque `word` — there are no operand nodes to patch, and the
converter synthesises the `$(( … ))` wrapper from raw text. So
`arith_text_dispositions` walks the text, rewriting identifiers, and
skips anything already carrying `$`/`{` and anything followed by `#` (a
bash base prefix, `16#ff`, not a read).

## The bug I wrote

The `${i}` arm first tested `node.named_child_count() == 1`. So does
`${#s}`. The converter dutifully replaced the whole expansion with
`${s:?s is not an integer}` — **dropping the length operator**, so the
program went from "length of s, plus one" to "s as an integer, plus
one". Silently. It compiled.

Two existing specs caught it (57.124, 57.126), which is the system
working. The fix tests the node *text* against `${` + ident + `}`, which
is what "a bare braced read" actually means; `is_bare_braced_read` names
it. Pinned again as 57.154 in the arithmetic context specifically, with
the failure mode in the comment.

The lesson is the same one as journal 318's carve-out: a predicate that
is *structurally* true of the thing you want is not the same as a
predicate that is true of *only* that thing.

Eight new specs, 57.148–57.155. Sweep unchanged elsewhere.
