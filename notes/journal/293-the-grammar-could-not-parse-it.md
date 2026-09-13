# 293 — The grammar could not parse what I was converting

`converter-braced-subscript-index` was a P3 with four gates and looked like an
afternoon of pattern matching. Three of the gates were. The fourth taught me
something about where the converter's real limits are.

## What DD#80 asks of the converter

Bash has one subscript rule: whatever is between the brackets is an arithmetic
expression. `${a[i]}` reads `i` as a variable and uses its value. `${a[i+1]}`
adds. `${a[a[0]]}` recurses. The rule is uniform, which is exactly what makes
it dangerous — `${a[$(rm -rf /)]}` is also an arithmetic expression under a
permissive enough reading, and bash's arithmetic evaluator has historically
been a reliable source of injection surface.

DD#80 takes the other road: a subscript is a **direct key**. An integer
literal, a `$var`, an expansion, a quoted string. Nothing is evaluated. If you
want arithmetic you write `$(( … ))` and say so.

That leaves the converter holding the difference, and it was not holding it.
Every bash subscript idiom passed through verbatim and then failed to compile
— SB4030 for a bare name, SB4041 for anything more complicated.

## Three gates that were what they looked like

The classification is small. A key is already direct if it starts with `$`, or
a quote, or is an integer literal, or is `@`/`*`. A bareword is a variable read
on an array (`$i`) but a literal key on a map. Anything else is an expression
and gets `$(( … ))`.

The map/array split is the only part that needs knowledge beyond the
expansion's own text, so `CollectionPass` — which already walked the script to
find out which names are collections — now also records which of them were
declared `-A`. Eight lines.

## The gate that was not

`${a[a[0]]}` should become `${a[${a[0]}]}`. My helper handled it; I could
follow the logic by hand and get the right answer. The converter emitted the
input unchanged.

I spent a few minutes assuming I had a patch conflict, because that is the
kind of thing that silently drops a rewrite. Then I dumped the CST:

```
string ["${a[a[0]]}"]
  ERROR [${a[a[0]]}]
    ${ [${]
    subscript [a[a[0]]
      ...
```

**tree-sitter-bash 0.25 cannot parse a nested subscript.** Not "parses it
oddly" — it produces an `ERROR` node where the `expansion` should be. My code
was never called, because `walk_node` dispatches on `node.kind()` and the kind
was `ERROR`.

This is worth sitting with. The converter's whole design rests on the grammar
handing it a faithful tree, and here the grammar is handing it a shrug for a
construct that bash itself accepts without complaint. Every rewrite the
converter performs is invisible to constructs the grammar cannot see, and
nothing announces which those are. The only reason I found this one is that I
had written a test for a specific output string and it did not appear.

The fix is unglamorous: an `ERROR` arm that checks whether the span's raw text
looks like `${name[…]}` and, if so, runs the same rewrite off the text. It
returns early so the walk does not descend into the malformed children and
patch them into an overlapping mess. It is a text-level fallback underneath a
tree-level tool, which is not something I would propose as an architecture —
but the span boundaries are exact even when the interior is not parsed, and
that is enough.

I do not think this is the only construct in that category. It is the only one
I happened to write a test for.

## A false positive found by accident

While dumping output I noticed `${a[@]}` drawing this:

```
# MANUAL: ${var@X} parameter transformation not supported in sash
```

It is not a parameter transformation. It is a splat. The check was
`inner.contains('@')`, and `a[@]` contains `@`.

I stashed my changes to confirm it predated me — it did. So the converter has
been telling people to hand-convert the single most common array idiom in
bash, on every script that uses one, for as long as that check has existed.
Nobody noticed because a MANUAL marker is advisory: it does not fail anything,
it does not change the output, it just sits there being wrong. The strictly
mechanical part of the converter is covered by specs. The advisory part is
what a human reads, and it was the part with no test.

`is_param_transform` now requires the `@` to be preceded by a name and
followed by exactly one letter. Spec `57.92` pins it.

## What the fourth gate could not reach

`${a[i+1]}` now converts to `${a[$(( i+1 ))]}`, which is correct. It also does
not compile, because `i=1` in bash is a *string*, and sash requires a
string→int coercion to carry a disposition — `${i:-0}` or `${i:?msg}`.

I nearly picked one. `${i:-0}` makes the output compile and is what bash
effectively does. But that is a value judgement wearing a mechanical
transformation's clothes: it converts a script that would have died on
malformed input into one that quietly computes with zero, and it does so
silently, at scale, in the tool whose entire purpose is moving people toward
saying what they mean. The alternative, `${i:?}`, is honest but diverges from
bash on exactly the inputs that matter.

So it is filed (`converter-string-var-arithmetic.md`), with a fourth option I
like better than either — infer `int i=1` when every assignment to the name in
the script is a numeric literal, which invents nothing and diverges from
nothing, and fall back to a `# MANUAL:` marker when inference fails. It is
also considerably more work, and it is DG's call.

The subscript work needs no revisiting when that lands. `${a[$(( i+1 ))]}` is
already the right answer; it fails today for a reason that has nothing to do
with subscripts.

## The pattern, again

Journal 292 ended on the observation that what survives in a mature codebase is
the population invisible to the checks the codebase performs on itself. Two
more for the pile, both found in a P3 cleanup task:

- A construct the **grammar** cannot represent, so no tree-walking pass can
  ever act on it, and no tree-walking test can ever miss it visibly.
- A diagnostic that is **advisory**, so being wrong costs nothing that any
  automated check measures.

Neither is exotic. Both were sitting in a file with 117 passing specs.
