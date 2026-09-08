# 256 — `len` on a glob array read 0 (DD#97)

DG: *"when I do `f=(*)` I see `$(len f)` return 0 despite the array having
elements."*

One report, two defects, and a third found on the way out.

## Reproducing found more than was reported

The first thing the repro did was widen. `f=(*)` gave 0 — but so did
`f=(a b c)`. So it wasn't about globs. Then narrowing the *other* way:
`array f=(a b c)` gave **3**. So it wasn't about bareness either. Both spellings
of the bug existed, for different reasons, and the report happened to sit on
the intersection where both applied.

Splitting them took one `eprintln!` in `try_len_intrinsic` printing
`state.collections`. That set is what decides `arrlen` (element count) vs
`strlen` (byte length of the slot); an unregistered collection gets `strlen` on
a table, which reads 0. The set was empty.

**Defect 1** — `track_collection_kind` inferred collection-ness from the
*lowered value*:

```rust
if matches!(val, SsaValue::AssocArray(_) | SsaValue::IndexedArray(_)) {
    state.collections.insert(name.to_string());
}
```

A glob lowers to `ParamExpansion { operator: "glob" }`. Not either variant, so
`array f=(*.txt)` — keyword and all — never registered. Which produced the
sharpest possible symptom:

```sh
array f=(*.txt)
$(join f)   # three paths
$(len f)    # 0
```

Two intrinsics, one variable, different answers. The fix is to register from
the **declaration keyword** rather than the value: the keyword is what the user
wrote, and it can't be defeated by however the value happens to lower. Inferring
a declared property from a derived artifact was the mistake; the property was
right there in the source.

**Defect 2** — `handle_plain_assign` never called `track_collection_kind` at
all. Only the declaration path did. So a bare `f=(a b c)` half-worked: element
reads and `for-in` fine, `len` 0, `join`/`unset` SB4030.

Half-working is worse than broken. A hard failure gets reported on day one; a
silent 0 gets built on.

## The question that wasn't mine

Defect 1 is unambiguously a bug — `join` and `len` contradicting each other is
not a design position. I fixed it.

Defect 2 is a language question: *should* bare `f=(…)` be an array? I put it to
DG with the measured table of what worked and what didn't, and the two options.
He chose reject — require `array`/`map`.

I'd recommended that, and the reasons held up better than my own first
instinct. DD#41 had *already* made `array arr=(…)` the declaration form; bare
`f=(…)` was never designed, it was an accident of the assignment path. DD#75
says the keyword form is canonical. And `f=(a b c)` reads as a scalar
assignment while silently producing a table — precisely the ambiguity the
project pivoted away from. Migration cost turned out to be zero: nothing in
`spec/` or `contrib/` used the bare form.

Worth noting the shape of the fork. "Make it work" and "reject it" both remove
the 0. Only one of them also removes the *ambiguity*, and that difference isn't
visible from inside the bug — you have to be looking at the language. That's
why it was DG's call and not mine.

## The third defect

`f+=(d)` on an undeclared name wasn't returning 0. It was a panic:

```
thread 'main' panicked at sash/src/codegen/types.rs:190:28:
ICE: undefined array: f
```

The reject fixes it for free, because SB5055 fires before codegen. This is
`ud.19` again — *"reading an UNDECLARED array `${z[0]}` is SB5031, not a
codegen ICE"* — on the write side. The read path had been taught this lesson;
the write path never was.

## Two corrections I owed

**The spec ids.** `g.8` failed and the harness said `len` was 3 when I expected
2. Not a compiler bug — `glob.sh` already creates a `g3.rr`, and I'd reused the
`.rr` extension, so `*.rr` matched a file from an earlier test. Same class as
the global-`chk`-namespace hazard: the spec directory has shared state, and I
assumed isolation without checking. Private extensions (`.g7x`, `.g8x`) fixed
it.

**Yesterday's DD.** DD#96 justified the comma with "`,` is already sash's
sequencing separator — array literals, argument lists, and `(( ))`." Working on
this bug I typed `array f=(a, b, c)` and got elements `a,` `b,` `c`. Array
literals are **space**-separated. So are map literals, and calls are shell-style.
`(( ))` was the only true item in a list of three I'd written as if
self-evident.

The decision survives — `(( ))` is the precedent the line-break rule is actually
borrowed from — but I'd inflated a one-precedent argument into a
"consistent-with-everything" argument, and I did it in the document that exists
to record *why*. Corrected in DD96, journal 255, and status.

That is the more useful lesson of this session. The compiler bugs were
mechanical. The doc error was a reasoning failure: I asserted a supporting fact
that would have taken ten seconds to check, in the one artifact whose whole
value is being trustworthy later.

## The janet find

`ud.29` — a *control* asserting that appending to a declared array stays legal —
passes native and c, prints `44` in janet:

```janet
(var f @["a" "b" "c"])
(set f (string f @["d"]))     # stringify both, concatenate
```

Scalar `+=` is string concatenation and is correct; the emitter applies the
scalar rule to a collection target without checking the kind. So the variable
*stops being an array*, and everything after is wrong, not just `len`.

Independent of DD#97 — `emit_janet` renders from the AST, not the SSA — so it
was pre-existing. Filed as `janet-array-append-string-concat` and left
**failing rather than `skip_chk`'d**. A skip in `glob.sh` means "janet doesn't
do globs," a whole missing feature. This is a defect in a feature janet claims
to have, and hiding it behind a skip would make the suite quieter and less
true.

## Sweep

4182 passing / 34 failing / 394 skipped. 0 regressions; +29 passing; the one
new failure is the janet defect above.
