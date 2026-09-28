# 442 — counting is not accessing

DG read the bug filing I left at the end of 441 and said:

> the example functions in the task file are not variadic (the use of len
> alone shouldn't qualify)

Both reproducers in `bug-call-arity-counts-compile-time-values` opened with

```sash
fn f {
  str a=${args[0]}
  str b=${args[1]}
  int n=$(len args)
  ...
}
```

and the file then asserted, in its own prose, that this callee was
`Fixed{min:2}`. It was not. DD#81 Part B put `$(len args)` in the whole-args
set, so the compiler had typed it `Variadic{floor:2}`.

The part that stings is that I had pasted the proof three lines down. The
quoted diagnostic read *"function 'f' expects **at least** 2 arguments"* —
and "at least" is the *variadic* message; the fixed one says "expects 2".
I copied a compiler error into a document to support a claim the error
contradicted, and did not read it.

## The reproducers did not need it

Strip the `len` and both directions reproduce against a callee that is
fixed-arity by every reading:

```sash
f one ${x:~}    # compiles, delivers 1   — and `f one` is SB5042
f @{q:-}        # q=(p d), rejected "called with 1"
```

Better, in fact: the messages now say "expects 2", so the file argues about
a fixed-arity miscount using a diagnostic that agrees it is fixed-arity. A
newly-measured detail went in too — a *three*-element splat gets the same
"called with 1", so the count is the constant 1 for any splat, not merely
imprecise. Committed as `83c1953`.

## The rule underneath

I checked `notes/decisions/` before touching anything, and reported back that
the behaviour was deliberate: DD#81 Part B states "`$(len args)` → variadic"
three separate times, accepted with DG on 2026-08-25. I also measured what
changing it would cost — 17 native specs — and asked.

DG's answer settled it:

> `$(len args)` is how a function would loop over variadic args; but
> accessing an arg via a loop or branch is what would make it variadic.
> Without the loop/branch the len is vacuous.

That is the distinction the original rule missed. `len` appears in variadic
code as the **loop bound**; the thing that actually makes the function
variadic is the **access** inside the loop — `${args[$i]}`, a dynamic index,
which `analyze_arity` already handles on its own. So `len` was never carrying
the signal. It was standing next to the thing that was.

Which means the old rule bought nothing and cost a ceiling:

```sash
fn f { str a=${args[0]}; str b=${args[1]}; echo "$(len args) [$a][$b]" }
f p d x y      # → "4 [p][d]"      accepted; two arguments silently discarded
```

Delete the `len` and the same call is SB5043. One mention of the count,
anywhere in the body, turned `Fixed{2,2}` into "any count".

## Verifying the rule before believing it

The claim that has to hold is that no *real* variadic idiom depends on `len`.
Patched, rebuilt, and checked each one:

| idiom | marked variadic by | survives |
|---|---|---|
| `while (( i < n )); do … ${args[$i]} …` | dynamic index | yes |
| `for x in @args` | splat | yes |
| `$(join args)` | whole-args fold | yes |
| `${args[1]:-}` optional trailing (DD#84) | — (`Fixed{1,2}`) | yes |

Then the 17 failures, every one pulled up and read. All 17 are the same
shape — **a body that observes the count and accesses no argument**:
`echo $#`, `echo $(len args)`, `$(( $# * 2 ))`, `if (( $# < 2 ))`,
`${arr[$#]}`. Not one is a variadic loop. The rule change breaks exactly the
vacuous class and nothing else, which is what DG said it would.

And the recovery was already in the language. DD#81's no-op idiom, unchanged
since 2026-08-25:

```sash
fn n { : @args; echo "$#" }
```

SB5043 was *already worded* to point at it — "use @args to accept a variable
number". The error a bare counter now gets tells you the fix.

## What landed

Two lines of compiler change (`cmdsub_reads_whole_args` drops `len`;
`sig_record_name` drops `#`), the DD#81 addendum, 17 specs migrated by adding
`: @args`, and 13 new specs (81.17–81.29) covering both halves: the bare
counter is rejected, `: @args` restores it, the fixed-arity ceiling survives a
`len`, and all three genuine variadic idioms still work without one.

Sweep 6835 / 6822 passing / 13 skipped / zero failing, total exactly as
predicted, zero flips against `5d15780` and nothing retired.

## What I did not change

DG said "a loop **or** branch". A conditionally-read *literal* index
(`if …; then str b=${args[1]}; fi`) still contributes a required fixed index
rather than going variadic, because DD#84 already gives that exact shape a
tighter spelling — `${args[1]:-}` → `Fixed{1,2}`, which states the optionality
instead of discarding the ceiling. Taking "branch" literally would replace a
two-sided bound with none. Left alone, flagged in the DD and to DG.

## The lesson, and it is not the one from 441

441 ended with "read the DD before writing 'this is blocked on DG'". This
session I did read it, and reported it correctly. The failure here was
earlier and dumber: **I quoted evidence without reading it.** The word "least"
was sitting in the middle of my own paragraph, disagreeing with the sentence
around it, through a commit and a summary.

A diagnostic pasted into a note is not decoration. It is the compiler's
opinion about the exact program under discussion, which is more than I can
say for the prose around it — so when the two disagree, the prose is wrong.
