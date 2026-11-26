title: Microsecond Unix epoch values
date: 2026-11-26

# Microsecond Unix epoch values

DG requested **"I need a microsecond version of epochseconds, maybe
epochuseconds"**. The AI used the suggested name and the existing
integer-intrinsic path, on `err-attempt-trial`. Main and DD136's provisional
adoption boundary are unchanged.

```sash
int now=$(epochuseconds)
int next=$(( (epochuseconds) ))
echo $now $next
```

The result is integer microseconds since the Unix epoch. Each invocation
samples `CLOCK_REALTIME` once, then computes
`seconds * 1000000 + nanoseconds / 1000`. Sub-microsecond precision is
discarded. This is wall-clock time: clock corrections may move it backward;
it is neither a monotonic timer nor a promise of distinct successive values,
microsecond accuracy or microsecond clock resolution.

## Existing machinery, new clock

The runtime returns `i64` directly. The conversion uses an `i128`
intermediate and a checked narrowing, so the complete mathematical result
is range-checked rather than allowing overflow or rejecting an intermediate
product that the fractional part could bring back into range. Overflow takes
the existing arithmetic-abort path.

The name joins the existing integer-denotation, nullary-arity, value-only,
clean-value and arithmetic-lowering tables. The native extern has the
same nullary-to-i64 signature as `epochseconds`; both native and C use their
existing shared integer producers, with string rendering delegated to them.
The malformed-capture arity unit's existing matrix includes the new name.
No parser, AST/SSA variant, dependency, magic variable or converter rewrite
is needed. `epochseconds` is unchanged.

DD58 and the specification outline describe the new clock. There is no
new architecture requiring a GUIDE section.

## Focused evidence

`epochus.1`-`.7` are fresh in both source and historical spec inventory:

| Claim | Specs |
|---|---|
| Realtime microsecond bounds for integer capture, sexpr, bare arithmetic, quoted text and an integer helper; seconds remain seconds | `epochus.1` |
| Stage-local evaluation and string rendering without taint bypass | `epochus.2` |
| Repeated live samples and a genuine subsecond component | `epochus.3` |
| Extra operands rejected in capture and arithmetic forms | `epochus.4`-`.5` |
| Explicit stringification and value-only placement | `epochus.6`-`.7` |

The bounds use the Bash harness's independent `EPOCHREALTIME` readings
around the actual CLI invocation. All seven assertions fail on the frozen
original compiler/runtime at `4388dcfd`; all 506 pre-existing type
assertions pass there. The final type file passes 513/513 in each backend.

Four independent reversions exercise the actual spec block:

| Reversion | Native failures | C failures |
|---|---|---|
| Remove the runtime nanosecond contribution | `.1`, `.2`, `.3` | `.1`, `.2`, `.3` |
| Route only the native new name to the seconds runtime call | `.1`, `.2` | None |
| Route only the C new name to the seconds runtime call | None | `.1`, `.2` |
| Omit the new name from shared arity metadata | `.4`, `.5` | `.4`, `.5` |

For the whole-second mutation, `.3` is the deterministic precision witness;
the outer time bounds can also catch it, depending on whether compilation
crosses a second boundary. They caught it in both measured runs. Every
mutation is restored. Source, compiler, runtime, lockfiles and protected
local files match the frozen positive hashes; the restored seven-spec block
passes in both modes.

## An existing pending guard, not a clock-specific rule

The initial eighth assertion expected `echo $(- epochuseconds)` to be
rejected. Its native xtrace instead showed successful compilation and a
timestamp. The frozen original compiler also accepts
`echo $(- epochseconds)` through `--emit-c`.

General statusless-intrinsic suppression enforcement is already an open
owner, [reject-statusless-intrinsic-suppression](../tasks/reject-statusless-intrinsic-suppression.md).
The initial assertion demanded that unrelated pending implementation; it
was removed from this feature's final suite, and the existing owner now
records the clock examples. No clock-only exception or broad suppression
repair is introduced. The discarded candidate was never entered into
spec history, so it creates no retirement.

The additional standalone runtime Clippy check is also not reported as
passing. It rejects 49 public raw-pointer FFI sites. Running the same check
on an isolated archived `4388dcfd` runtime with the same lockfile produces
the same 49 errors and 66 warnings. These precede the eight-line clock
addition; no unrelated runtime API cleanup or lint suppression is included.

## Ordinary gate

The first full gate passes with the original prediction:

```sh
NEW=14 FLIPS=0 RETIRED=0 TOTAL=13240 timeout 900 mk -p 1 gate
```

The sweep takes 342 seconds and writes rows 7517533-7530772: 13240 checks,
13227 passing and 13 existing skips, native 6736 and C 6504. The gate reports
exactly 14 first-ever IDs, no flips, retirements, ghosts or plan mismatches,
and a nonvacuous liveness count of 246. Its `mk test` phase passes the
240 compiler, 27 converter and 20 expander units.

An independent comparison against the preceding accepted block
7504307-7517532 confirms all 13226 retained statuses are unchanged and the
only additions are the seven `epochus` IDs in each mode. The comparison is
by row blocks, not by a shared pre-commit HEAD hash.

Private evidence is retained in the session's
`files/epochuseconds-20261126/`: before/positive compiler-runtime pairs,
source/protected-file hashes, full-file and focused TSVs, reversion logs,
the two runtime-Clippy results and the gate log. No fresh coverage run is
part of this task.
