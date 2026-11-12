title: "bug: `a =~ s/…/…/` on an ARRAY silently empties it; `gsub @a …` silently joins the splat"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`array a=(x:1 y:2); a =~ s/:/-/g` is REJECTED with an SB code — no silent `len 0`, no segfault"
  - "`$(gsub @a \":\" \"-\")` is likewise rejected — it must not silently behave as `$(gsub \"$(join a)\" …)`"
  - "a spec pins the rejection for both the statement (`=~`) and function (`$(gsub …)`) forms"

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

## Decided 2026-09-10 (DG): **reject** (option B)

Re-verified at HEAD, and it is worse than first filed — not merely silent
emptying:

```
before n=2 [0]=x:1
after  n=0 [0]=            <- then Segmentation fault (core dumped)
```

`$(gsub @a ":" "-")` returns `x-1 y-2`, i.e. it silently behaves as
`gsub "$(join a)" …`.

Cause: DD#96 defines `v =~ s/RE/SUB/` as `v = $(sub "$v" RE SUB)`
(`parse_regex_mutation`, parse/parser.rs:1949). Applied to an array that is a
category error — a scalar is assigned to the array slot, `len` reads 0, and
the following `${a[0]}` reads through a dangling table.

**Both forms are rejected.** A string operation does not apply to a
collection; the author writes the loop. Mapping over elements (option A) was
considered and deferred, not dismissed — see
`future-array-element-mapping.md`.

Scope when implemented: the statement form is rejected where the desugaring is
built (`parse_regex_mutation` knows the target name, so it needs the
declaration set), and the value form where `@a` reaches a scalar operand of
`sub`/`gsub`. The segfault goes with it — but check separately whether
`${a[0]}` after a bad assignment can still fault, since that is the more
general defect.

## Resolved 2026-09-11 — SB5064 (a) + SB5070 (b) (journal 265)

**(a)** closed earlier by SB5064: `a =~ s/…/…/` on an array is rejected at the
scalar-sigil read the desugaring produces. The follow-on `${a[0]}` fault noted
in the scope paragraph is SB5068, also landed.

**(b)** is SB5070. Arity checked out because a splat is one *syntactic*
argument, so `check_cmdsub_arity` counted three and passed; the splat then
collapsed to a joined string at runtime.

**Scope widened on measurement.** This filing named `gsub`; all seven
scalar-operand value intrinsics had the identical defect — `sub`, `gsub`,
`toupper`, `tolower`, `basename`, `dirname`, `match`. Fixing only the filed
one would have left five silent wrong answers of the same shape behind a green
suite, so SB5070 covers all seven, in every argument position, bare splat or
carrying a disposition.

`len`, `join` and `keys` are excluded on principle, not expediency: they take a
collection **by name**, so `@a` there is not a reinterpretation (`len @a`
correctly returns the element count; `keys @a` is already SB5025). `len @a` /
`join @a` remain DG's open DD#75 questions.

The message names `$(join a SEP)`; spec 96.20 runs that advice to confirm it
compiles and yields what the author was reaching for.

Specs `var_modifiers.sh` 96.12–96.22.
