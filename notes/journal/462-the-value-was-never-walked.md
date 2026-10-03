# 462 — The value was never walked

DG ruled on the fork I filed at the end of 461:

> I don't think `arr[$(f)]` is legal, but `arr[$((f))]` where `f` is an int
> returning fn seems fine.

Which is the answer I expected and could not give myself, because it is a
statement about the language rather than about the code.

## The rule never moved

What makes this one worth writing down is how little had to change. DD#80 §2
already reads *"an indexed array's index must be int-typed"*, and the
implementation of that is one line:

```rust
is_integer_literal(expr) || expr_type(expr, env) == VarType::Int
```

A `$( … )` types as String. So `is_int_index` had been answering DG's question
correctly, in those exact words, since the 2026-09-26 amendment wrote it. It
just was not being asked.

`check_assignment` and `check_declaration` walked their **target's** subscript
and stopped. Nothing walked the **value**. A command walks its arguments, so
`echo "${arr[$(f)]}"` was SB4030; an assignment did not, so
`str z=${arr[$(f)]}` compiled and printed `q`. One expression, two answers,
decided by which side of the `=` it sat on.

I had already written the one-line fix in 461 and reverted it, because it
regressed `functions.sh` lift.3 — a spec that asserted the acceptance. That
was the right call at the time and I want to be precise about why: a passing
spec is evidence that *somebody decided this*, and I cannot tell from the
spec whether that somebody meant it. Only DG can. The filing existed to ask.

The answer: lift.3 was pinning a hole. It is now a rejection, with
`lift.3b`/`lift.3c` pinning the form DG named.

## The ICE was the same missing walk

The same absent value-walk was the cause of a panic I had found while chasing
something else:

```
str v="${a[foo]}"
→ panicked at codegen/values.rs:1426:
  non-integer literal `foo` reached int lowering;
  type_check (SB4030) should have rejected it
```

The assertion is the compiler naming, precisely, the pass that never ran. I
confirmed on a pristine HEAD binary that it predated my work. It is closed
now, by the same four lines — not because I went after it, but because there
was only ever one defect and it had two symptoms.

That is the part I would not have predicted from the filing. I wrote
`capture-as-subscript-index-position-dependent` as a language question and
`${a[foo]}` up as a separate ICE. They were the same missing walk.

## The diagnostic had to name a form that works

`array_index_error` carries a standing rule in its own doc comment:
*"recommending `${a[$i]}` to someone who wrote `${a[$s]}` sends them from one
SB4030 to the same SB4030."* The generic index message advises an integer
literal, an `int` variable, or `$(( … ))`. For a capture, all three are dead
ends — I measured them:

| what the author would try next | result |
|---|---|
| `${a[$(( $(f) ))]}` | **SB4041** — a capture is not an arithmetic operand |
| `int i=$(f)` | **SB4030** |
| declare `fn f -> int`, then `${a[$(( (f) ))]}` | works |

Exactly one migration exists, so the capture arm names it directly. Had I
shipped the generic message, every author who hit this would have taken two
more compiles to find the only thing that works.

## Sweep

7417, exactly as predicted (7395 + 2×(9 types + 2 functions)), zero failing.
`mk test` green. The literal in the new diagnostic needed repairing first —
a non-raw Python heredoc ate the Rust `\`+newline continuations and left runs
of fourteen literal spaces in the message. Worth a note: write Rust string
continuations through `r'''…'''` or not through Python at all.

## Where I stopped

DD#80 gained an addendum recording the ruling and, more usefully, recording
that this is the **second** time its enforcement turned out to be
position-dependent rather than rule-dependent. The first was the write
surface, fixed in the 2026-09-26 amendment by making both surfaces call one
parser. Same shape, same non-fix needed to the rule, same missing call.

Still open for DG: `suppress-rewalk-of-errored-subtree` (his wider question
from 461), `taint-sb5067`, and the three tasks awaiting honest re-estimation.

Next is the interrupted coverage thread. `coverage.db` is stale —
`type_check.rs` has been renumbered three times in two sessions and that
database is keyed by `(file, line)` with no commit recorded — so `mk cov-union`
and `mk cov-db` have to run before any of it means anything.
