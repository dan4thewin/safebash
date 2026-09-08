---
title: "bug: `a =~ s/…/…/` on an ARRAY silently empties it; `gsub @a …` silently joins the splat"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`array a=(x:1 y:2); a =~ s/:/-/g` either maps over the elements or is REJECTED with an SB code — it must not silently leave `len 0`"
  - "`$(gsub @a \":\" \"-\")` either maps over the elements or is rejected — it must not silently behave as `$(gsub \"$(join a)\" …)`"
  - "whichever is chosen, a spec pins it for both the statement (`=~`) and function (`$(gsub …)`) forms"
---

## Found

2026-09-08, while weighing `readline`-to-array vs `readline`-to-scalar: the
question turns on how well arrays can be reshaped in-process, and the answer is
that reshaping them is currently broken.

## (a) A sed op on an array silently destroys it

```sh
array a=(x:1 y:2)
echo "before n=$(len a) [0]=${a[0]}"   # before n=2 [0]=x:1
a =~ s/:/-/g
echo "after  n=$(len a) [0]=${a[0]}"   # after  n=0 [0]=
```

No diagnostic. The array is emptied. Silent data loss.

DD#96 defines `v =~ s/RE/SUB/` as desugaring to `v = $(sub|gsub "$v" RE SUB)`.
On an array subject, the desugaring presumably produces a scalar assignment to
a collection name, which lands as an empty table. Whether an array subject
should **map over elements** or be **rejected** is a design question — silently
emptying is not a candidate answer.

## (b) A splat argument to `gsub` is silently joined

```sh
array a=(x:1 y:2 z:3)
t=$(gsub @a ":" "-")
echo "[$t]"                            # [x-1 y-2 z-3]  — ONE joined string
```

`gsub` has a strict arity (`$(gsub $s ":" "-" "EXTRA")` and `$(gsub $s ":")`
are both **SB4045**), so a 3-element splat plus two more args should be five
arguments and rejected. Instead the splat collapses to a single joined
argument and the call succeeds.

That contradicts DD#75, which makes `$(join a)` the explicit way to get a
joined string, and DD#22/#69, under which a splat yields one argument per
element. The user asked for something the language does not define, and got a
silent reinterpretation rather than a diagnostic.

## Why it matters beyond the two cases

These are the only in-process routes for reshaping a collection. With both
broken, the only working reshape path is scalar-shaped:

```sh
sep=$'\0'
s=a:b:c
s =~ s/:/$sep/g       # works — on a SCALAR
```

That asymmetry is live input to the `switch-free-read-family` decision (whether
`readline` targets an array or a scalar): an array-only read model wants
in-process array reshaping, and it does not currently exist.

## Note

Both compile cleanly and produce wrong results — the recurring shape of this
week's finds (DD#97's `len`/`join` disagreement, the DD#87 execve leak, the
taint laundering): not a missing feature, an **accepted input with an
undefined answer**.
