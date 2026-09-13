---
status: blocked
priority: P2
kind: convert
depends:
gates: DG decides the converter's disposition policy for a string variable used in arithmetic
---

# The converter emits non-compiling output for `$(( strvar + 1 ))`

Found while implementing `converter-braced-subscript-index`. That task is
otherwise complete; this is the one case it cannot reach.

## The measurement

```sh
$ cat in.sh
i=1
echo $(( i + 1 ))

$ sash-convert in.sh
i=1
echo $(( i + 1 ))          # verbatim — nothing to rewrite

$ sash -o out out.sash
out.sash:2:9: error [SB4030] a string→int conversion must carry a
              disposition: `${i:-N}` or `${i:?msg}`
```

Bash has one scalar type, so `i=1` is a string that arithmetic silently
coerces. Sash requires the coercion to be spelled, because "what if it isn't a
number" is a real question with two defensible answers. The converter today
answers neither: it emits the bash spelling, which does not compile.

This is not specific to subscripts. Every bash script that assigns a numeric
literal without `declare -i` and then does arithmetic on it hits this — which
is most of them. `converter-braced-subscript-index` only surfaced it because
`${a[i+1]}` converts to `${a[$(( i + 1 ))]}`, inheriting the same failure.

## Why this is DG's call

Each option changes the *semantics* of the converted program in a way the
converter cannot derive from the source. Per DD#66 the converter may flag
ambiguity, but choosing a disposition is choosing behavior.

**(a) Silent default — emit `${i:-0}`.**
Converted scripts compile and run immediately. But it invents a value: a
script that would have died on malformed input now quietly computes with
zero. That is exactly the failure mode the disposition requirement exists to
prevent, reintroduced by the tool that is supposed to be migrating people
toward it.

**(b) Abort default — emit `${i:?not a number}`.**
Preserves the "bad input is an error" reading and compiles. But bash does
*not* abort here (it treats a non-numeric string as 0), so the converted
program diverges from the original on exactly the inputs anyone cares about.
Byte-identical-to-bash is the project's own standard.

**(c) `# MANUAL:` marker, leave the arithmetic alone.**
Honest: it says "this needs a human decision" and does not guess. Matches the
existing convention for `${!var}`, `case`, subshells, and process
substitution. Cost: the converted script does not compile until the human
acts, and this construct is common enough that a large script could carry
dozens of markers.

**(d) Infer `int i=1` from a numeric-literal assignment.**
The most faithful option when it applies — if every assignment to `i` in the
script is a numeric literal, `int i=1` is what the author meant, the
arithmetic needs no disposition at all, and nothing is invented. Cost: it is
a whole-script dataflow pass, not a local rewrite, and it silently changes the
variable's type (a later `i=abc` becomes a compile error rather than a
runtime surprise — arguably an improvement, but a behavior change). Falls
back to (c) for anything it cannot prove.

## Recommendation

(d) with a (c) fallback. It is the only option that neither invents a value
nor diverges from bash, and the fallback keeps it honest where inference
fails. It is also the most work by a wide margin, so (c) alone is a
defensible first step that can be tightened later — (c) → (d) is a pure
improvement with no compatibility cost, since it only converts markers into
working code.

## What is already done

`converter-braced-subscript-index` is complete and shipped. `${a[i+1]}` now
converts to `${a[$(( i+1 ))]}`, which is the correct sash spelling — it
compiles as soon as `i` is an int, and fails identically to a bare
`$(( i+1 ))` otherwise. No part of the subscript work needs revisiting when
this is decided.
