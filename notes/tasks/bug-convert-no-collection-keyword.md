title: "bug: sash-convert never introduces the DD#97 `array`/`map` keyword"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "bash `a=(x y z)` converts to a program that compiles"
  - "`a[0]=x`, `mapfile -t a` and `read -a a` likewise"
  - "a name already covered by `declare -a`/`-A` does not get a second keyword"

## What

DD#97: *a collection is introduced by the `array`/`map` keyword*. bash has no
such requirement — it declares implicitly, in at least four ways:

```sh
a=(x y z)          # array literal
a[0]=x             # subscripted assignment
mapfile -t a < f   # fills an array
read -a a          # fills an array
```

`sash-convert` converted every one of them to itself, with no keyword. The
result does not compile:

```
error [SB5055] `a` is given a collection value without a collection
declaration — declare it `array a=(…)` …
error [SB5068] `a` is a scalar, so `a[…]` has no element to address …
```

Only the *explicit* bash declarations were handled: `declare -a b=(p q)` →
`array b=(p q)` and `declare -A m` → `map m` both worked. Every implicit form
— which is how most real bash scripts create arrays — produced output that
failed at the next step of the pipeline.

## Found

2026-09-13, while fixing `bug-convert-hash-array-conflation`. That task's
gate is that `${#a}` on an array converts to the length of element 0, and
demonstrating it end-to-end means compiling the converted program. It did not
compile, for this unrelated reason.

Three separate symptoms (`a=(…)`, `a[0]=x`, `mapfile`) turned out to be one
defect with one cause, so they were fixed together rather than filed apart.

## Fix

`CollectionPass` in `sash-convert/src/convert.rs` walks the tree in source
order — pre-order recursion *is* source order, which is what "first mention"
means — and introduces the keyword once per name, at the first construct that
mentions it:

| bash | emitted |
|---|---|
| `a=(x y z)` | `array a=(x y z)` — a zero-width insert, so patches on words *inside* the literal still apply |
| `a[0]=x` | `array a` on its own line (the keyword cannot go inline) |
| `mapfile -t c` / `read -a c` | `array c` on its own line |
| `declare -a`/`-A` | nothing — `convert_declaration` already supplies it |

The same pass collects the collection names that `length_expansion` needs, so
one walk serves both.

## Known gap

A name first assigned inside one branch of an `if` gets the keyword there, and
the other branch's assignment does not. bash has no declaration to mirror, so
any choice here is a guess; the common straight-line case is right and the
failure mode is a compile error, not wrong output. Not filed separately —
raise it if a real script hits it.

## Specs

`57.80`–`57.83` (each form) and `57.85` (round trip: convert → compile → run →
compare with bash) in `spec/convert.sh`. All five fail on revert.
