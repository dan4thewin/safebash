# 516 - Loop counters are integers

2026-10-16/17. DG: "continue with coverage".

Started at `a6fb024` with current coverage provenance, 247 compiler production
cold lines, and nine in `codegen/values.rs`. Took the whole native-value
cohort, including its producers and the corresponding C paths.

## Loop control was allocating strings

Synthesized array/map indices, argv indices and argv bounds were absent from
`VersionState.type_map`. They defaulted to String. Every increment formatted
the counter, and each integer use parsed it back:

```sash
array a=(red blue)
int n=0
while (( n<10000 )) {
    for x in @a { : $x }
    (( n++ ))
}
echo $n
```

With `--heap-size=64K`, both backends exhausted the heap. A matching int-only
loop completed and printed `10000`; argv-slice loops failed the same way.
This was a runtime correctness bug, not merely an unattractive C listing.

The counters and argv bound now reserve Int names. C also lowers table lengths
and integer intrinsic captures directly, instead of obtaining a string and
parsing it. One C intrinsic emitter serves both integer and string contexts;
only the latter formats. Primitive results are snapshotted in fresh i64
temporaries, preserving one-time evaluation.

Five small-heap CLI regressions cover array values, map values, map keys,
argv slices and empty arrays. Four C-shape specs require no `atoi`/`itoa` in
the generated main for those control paths and primitive integer captures.
Before controls failed on four of the five native heap cases, all five C
heap cases, and all four shape cases. The native empty-array case already
passed; C was still formatting its bound.

## A compiler name is not a reserved author name

The isolation check found a second bug:

```sash
str __ai1=keep
array a=(x)
for x in @a { echo $x }
echo $__ai1
```

The saved before compiler printed `x` and then `1`, overwriting the author's
binding. An argv example overwrote `__fi1` with `3`. Simply changing the
generated name's type turned the existing miscompile into a mixed-slot ICE.

The first allocator checked the inferred type map. That protected current and
future declarations, but it was not a complete namespace: a final probe using
`for __ai1 in @a` still looped indefinitely. `infer_types` does not record
implicit loop binders. Nor does it record every command-introduced binding.

`VersionState.reserved_names` now starts with inferred names and reserves
source names before lowering. The canonical statement/expression walkers reach
functions, captures and END; an exhaustive metadata visitor records assignment,
declaration and loop names. Literal operands are deliberately over-reserved
instead of duplicating read/argv/attest binder argument grammars. Arithmetic
attest conversion targets use the existing canonical target classifier.

All three loop temporaries use that one pool, append `_` until free, reserve
the chosen name, and separately register its Int type. The pool records minted
names too; there is no second, redundant type-map collision check.

Seven dual-mode isolation specs pin existing and future declarations, multiple
conflicting suffixes, each implicit counter-family binder, a later word-loop
binder, and a conversion target inside an active collection loop. Internal
looking prefixes are not assumed private.

This is a small internal allocator, not a new source restriction or namespace.

## Remove the impossible relay, share the real guards

Every native caller supplies the exit-status variable. Enumerated the explicit
omissions and input producers, then narrowed all **41** exit-status parameters
from `Option<Variable>` to `Variable`. Removed impossible omission branches
from `$?`, intrinsic status, getenv lookup status, and capture abort handling.

Index lookup now uses the canonical `var_slot` guard, and integer slot loading
is shared rather than keeping a second String/Int conversion match. Command
splats and array-literal splats share their effective-table resolver.

Typing the counters exposed a newly dead String `ArithExpr` implementation.
Enumerated every production constructor: exactly four, the two lowering
passes of collection and argv loop increments. General source arithmetic uses
`ArithBinOp`. Both backends now route String-slot loop arithmetic to the
existing shared scalar-lowering invariant instead of silently formatting it.
Its claim is explicitly about integer storage, not about removing the node.

The native API tests fabricate malformed SSA and invoke the existing
`compile_ssa`; they do not create a new FunctionBuilder harness or run source
through a bypassed compiler. They pin shared-guard routing, IntCell misuse,
an integer capture without an emitter, an invalid integer literal, and loop
arithmetic in a String slot. The C tests pin malformed integer captures,
String-slot loop arithmetic, and the direct status emitter's integer snapshot.

The IntCell test passes the rival check by execution. Temporarily replacing
the guard with zero made the unit fail because malformed SSA compiled, while
all **8585 CLI checks then present** passed. Restored the guard before final
measurements. The seven name-isolation specs were added afterwards.

Compiler-only spec profiles give an independent attribution check. The index
integer path executes **122** times. Native scalar refusal, IntCell refusal,
missing integer emitter and invalid literal remain cold there and execute
**2 / 1 / 3 / 1** times in the union. C's scalar refusal, direct status-emitter
arm and malformed capture head likewise stay at zero in CLI profiles and
execute **1 / 1 / 2** times in the union. These are tested API contracts,
not source gaps declared exempt.

## Arithmetic collection must preserve adjacency

The primitive C-shape probe used compact `ws!=0`, which unexpectedly rejected:
the collector reconstructed it as `ws! =0`. Its private operator-suffix list
did not include `!`, on the incorrect assumption that `!=` was always one shell
token. The same list made `((p- =2))` silently become subtraction-assignment.

The collector now preserves actual physical adjacency instead of privately
enumerating arithmetic operators. Added six dual-mode specs for compact
literal/variable/quoted/for-header inequality and separated-operator errors.
The numeric shape probe uses spaced operators to keep its claim independent.
Updated DD#94's enforcement notes; no new arithmetic rule was decided here.

## Accounting and measurement

Added **22 dual-mode specs**: control_flow 97 -> 109, cli 66 -> 70, arithmetic
466 -> 472. Both ID collision checks are clean. Historical reservations
preceded the additions; `loopint.10`/`.11` were above the historical high-water
mark of nine, and `.12`-`.16` followed the new mark of eleven. Predicted
**8599** checks, native 4402 / C 4197, and obtained
exactly that: **8586 passing / 13 existing C skips**.

Compared against saved prechange row boundary **5673023**, not a shared HEAD
hash: all **8555** baseline IDs match, with zero flips and exactly **44**
genuinely new IDs. Final reruns have zero new IDs and zero flips. Comparison
liveness is **260**. Two new shape IDs first failed while the probe exposed
the arithmetic bug; those account for the increase from 258, not regressions
in old specs. No unexpected retirements. The only plan/token discrepancy is
the established three-ID loop in coverage.sh.

Units **146 sash + 27 convert + 20 expand**. Refactored the touched complexity
violators; no bypass or hard-coded thresholds. The first commit attempt also
caught `emit_attest_all` at 5/7, touched by the mechanical status change.
Unified its duplicated value binding and loop step. Its all-passed flag already
starts true and only changes in the caught form, so completion can read that
flag in both forms instead of branching again. It is now 4/4, and every staged
formatting, clippy, totality and complexity gate passes. Source-name reservation
also touched the entry point's grandfathered 5/6 nesting: one guard-clause
function-registration pass replaces the separate name/int-return walks, taking
the entry point to 2/0 without changing forward-reference behavior. Repeated
the union after every final source change rather than retaining stale line
attribution.

Fresh serialized union, database and matching worktree-blob provenance:

| Measure | Before | After |
|---|---:|---:|
| Native value production cold lines | 9 | **0** |
| Compiler production cold lines | 247 | **234** |
| Compiler production recorded lines | 29430 | 29485 |
| Compiler production cold branches | 245 | 241 |

Parser, lexer, expr and arithmetic remain at zero cold production lines.
Raw cargo headline: **98.85% line / 99.35% function / 93.39% branch**.
The native-value file has 1271 recorded production lines, all warm.

Exported CLI-only profiles before removing instrumented artifacts. Restored
ordinary release binaries with `mk cov-reset`; a compiler canary with an
explicit profile destination emits nothing. Repeated ordinary units and the
full sweep, and checked spec.db rather than trusting run.sh's exit status.
The first failing development sweep had returned zero despite two failing
new IDs; the database exposed them.

Journal, status and DD#94 updated. No new architecture, language decision,
deferred work or SOP change.
