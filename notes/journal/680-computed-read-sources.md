title: Materializing computed read sources
date: 2026-11-28

# Materializing computed read sources

DG selected the computed-read-source defect from three backlog options.
The AI implemented the existing shared-materializer repair, without a new
language decision or changes to the operational script.
The [owner](../tasks/bug-read-source-capture-unlowered.md) closes.

## The missing step

The independent reproducer is:

```sh
fn f { read ${1:s/,/$'\x00'/g} _a; for entry in @_a { echo $entry } }
s=$(f warm)
echo $s
```

The expected output is `warm`, with normal taint checking. Sed sugar
discards the regex operation's match boolean, so the absence of a comma
must not abort or make the literal argument tainted.

`capture_value` represents this suppressed substitution as a
`CmdSubstBody` awaiting materialization. `lower_newread` called the shared
materializer only when a pending redirect scope existed. Otherwise it sent
the carrier straight to `SplitValue`, where the SSA backstop treated it as
tainted and the backends could not evaluate it.

The fix moves the empty-scope shortcut after
`unsuppressed(... lift_int_func_calls ...)`. Every explicit source now
uses the normal value path; an actual redirect scope still receives the
additional `retain_scalar` snapshot before evaluating its targets.
This reuses existing capture, expression-order and failure handling rather
than adding a read-specific walker. Splitting, private collection
publication, runtime code and both backends are unchanged.

## Regression proof

Five new cases in [wordsplit.sh](../../spec/wordsplit.sh) cover:

| IDs | Assertion |
| --- | --- |
| `readsrc.1` | The exact no-match positional reproducer prints `warm`. |
| `readsrc.2` | Comma substitution preserves empty fields and NUL terminator semantics, appends to an existing array, and leaves its source unchanged. |
| `readsrc.3` | A named capture runs once; earlier word parts retain their values and later parts see its global write. |
| `readsrc.4` | An anonymous source body runs once per loop iteration; scratch contains only that iteration's result. |
| `readsrc.5` | A caller's condition cannot catch an operand/body failure; ERR still sees the original destination population. |

The final legal fixtures fail exactly these five assertions on the frozen
`fa55ed7e` compiler in both native and C. All 147 existing word-splitting
checks remain passing. The repaired compiler passes all 152 per mode,
without taint bypasses in the new cases.

A final-tree reversion moves the shortcut back, rebuilds a compiler
byte-identical to the frozen original, and reproduces exactly the same
five failures per mode. Restoring the repair restores the exact passing
source and compiler identities. Separate original/repaired controls in
both modes retain an earlier source value across an executable redirect
target's global write, and reject a tainted capture with SB5020.

Two initial fixtures mistakenly named their helper `source`, which is
rejected by SB4001. Their traces identified the fixture error. The final
before/after and reversion campaigns use the legal name `produce`; the
earlier failures are not counted as defect evidence.

## Gate and scope

The predicted gate is:

```sh
NEW=10 FLIPS=0 RETIRED=0 TOTAL=13426 timeout 900 mk -p 1 gate
```

It passes on the first sweep: rows 7597625-7611050, native 6,829 plus
C 6,597, with 13,413 passing and 13 existing skips. The sweep takes 341
seconds. All 13,416 statuses from the saved acceptance baseline
(rows 7584209-7597624) remain unchanged; exactly ten IDs are added and none
retire. There are no ghosts or plan mismatches; liveness reports 246.
The gate includes passing 240/27/20 Rust unit phases.

Formatting passes. The touched function remains at cyclomatic 8 /
cognitive 3, below the live gate thresholds. The runtime archive, both
local lockfiles and unrelated `notes/dat` remain byte-identical.
No coverage run, dependency change, new architecture or DD revision is
needed. Broader evaluation ordering and negated-test buffer eligibility
retain their existing owners.

Private evidence in `files/read-source-20261128/` retains the frozen
compiler/runtime, protected hashes, final source patch, native/C traces
and TSVs, preservation controls, reversion builds and full-gate logs.
