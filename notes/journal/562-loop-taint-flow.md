# 562 - Complete loop taint flow

2026-11-02. DG authorized W3. I implemented the ordinary-loop transfers on
DD127's existing independent summaries, preserving W2's read boundary and
shared-output ownership. There is no caller-context walker, runtime change
or SSA-policy change in this unit. W4 and W5 remain separate.

## What was still broken

The current compiler, not the frozen reference trees, ran the 22 selected
historical reproducers in both modes. Four ordinary-loop defects remained:

- B95 lost the states of branches ending in `break` or `continue`.
- B98 applied a for binder to the state that also represented zero trips.
- B99 let a hypothetical while body erase the final condition's effects.
- B107 checked live collection words only against the initial state.

W1 already handles function returns, transitive slot effects, captured
outer names and value-call out-parameters. B102/B103's original single-use
fixtures correctly get SB5020 at their later reads. Adding a second `f`
call confirms shared-source reporting too: SB5067 at the for binder, 2:3,
and at the call to `h`, 6:3, on both the W3 baseline and final compiler,
in both backends. Those are W1 results, not new W3 fixes.

The reference supplied the loop-flow outline, not the architecture. Its
caller-context machinery was not imported. Its oversized-count fallback
to one was not imported either: existing lowering clamps outward, and that
behavior is preserved.

## Heads, back edges and exits

`BodyCx` now owns a stack of `LoopExits`. Each frame stores complete
`Tainted` break and continue deposits: both names and origins. A diverted
branch deposits before its ordinary fallthrough state is discarded.
Counted jumps select the actual target frame.

The head grows from the entry plus back-edge states until the complete
state stabilizes. The finite state has no arbitrary 64-pass cutoff.
Speculative passes use a private body context with the same enclosing loop
depth and empty deposits. They cannot export returns, jumps, ownership
obligations or source-log effects. They retain the existing summary-probe
flag rather than setting it merely to silence diagnostics: that flag also
controls adjacency measurement.

The retained depth matters. If an inner speculative walk starts at depth
zero, `continue 2` clamps to the inner loop instead of the enclosing one,
and a clean inner read gets a false SB5020. `loopflow.20` pins that case.

A while frame encloses its condition as well as its body. The next head
joins continues with body fallthrough. The exit joins breaks, the head and
the post-condition state; it cannot assume the body ran after the final
condition. The existing conservative syntactic reachability policy stays.

A for checks and evaluates its words at the stabilized head, then clones
that state for an entered iteration. Only that clone receives the fresh
binder. The pre-binding words state contributes to exit, representing zero
trips and the final failed head. Binding and adjacency are applied anew on
every iteration.

`ir::loop_jump_index` shares the existing default, positive-count overflow
handling and outward clamp with lowering. Empty probe/discarded stacks can
legitimately decline a target. This helper does not establish a body
boundary; the stack supplied by its caller must already have one.

## Specs and controls

`taint.sh` grows from 517 to 538 checks per mode. `loopflow` had no current
or historical IDs before this change.

| IDs | Claim |
|---|---|
| 1-4 | Break/continue state survives to an exit or subsequent iteration |
| 5, 9, 10, 21 | Zero trips, clean final values and per-iteration binding |
| 6-7 | Failed capture/readline conditions retain their effects |
| 8 | A live collection is checked at later heads |
| 11-13 | Two-level and oversized jump counts |
| 14 | A while condition belongs to that while |
| 15 | Ordinary nested-capture loop and enclosing cleanup preservation |
| 16-17 | Shared-output owners survive break/continue |
| 18-19 | Single-use summary propagation and original shared-source location |
| 20 | Quiet nested probes preserve enclosing target depth |

The authoritative `before-verified` runs fail seventeen new checks per
mode: `.1`-`.13` and `.16`-`.19`. Fifteen expected-refusal programs compile
and execute without diagnostics before the change; `.9` and `.10` instead
receive false SB5020 rejections. `.14`, `.15`, `.20` and `.21` already pass.
The initial implementation passes all 538 checks in each mode.

Earlier drafts are not those controls. Unused binders first triggered
SB5050 and were corrected with explicit noop reads. A counted-capture
draft exposed the separate lowering defect below; `.15` uses a plain
`break` and is a preservation case, not proof of counted-capture isolation.

Ten independent reversions were built and exercised serially in both
modes, with each restored before the next:

| Reversion | Red `loopflow` IDs in each mode |
|---|---|
| Drop break deposits | 1, 4, 11, 13, 16, 18, 19 |
| Drop continue deposits | 2, 3, 12, 17 |
| Use the bound body state for for exit | 5, 9, 21 |
| Omit fresh iteration binding | 10 |
| Drop post-condition exit state | 6, 7 |
| Check for words at the original entry | 2, 8, 12, 17, 21 |
| Start quiet probes at depth zero | 20 |
| Deposit names without owners | 16, 17 |
| Treat every jump count as one | 11, 12, 13, 20 |
| Walk a while condition outside its loop frame | 14 |

All twenty runs record exactly 538 checks and exactly the listed failures;
no older spec fails. Restored source hashes and compiler bytes matched the
saved positive. These controls precede the late invalid-count guard below;
they are not falsely labelled as runs of the final guarded compiler.

## The first full gate found an introduced ICE

The first prediction was `NEW=42 FLIPS=0 RETIRED=0 TOTAL=10824`.
The 243-second sweep, rows **6691741-6702564**, ran exactly that total and
recorded exactly 42 first-ever IDs, but failed six existing checks:
`cf.lc6`, `.lc7` and `.lc8` in both modes. They cover `continue 0`,
`break 0` and `continue 00`, which should receive SB5097.

Their xtraces and a preserved raw compiler invocation showed an
index-out-of-bounds panic. I had assumed SB5097 had already validated the
count. It had not: `run_semantic_checks` calls taint before
`check_loop_control_context`, collecting both sets of diagnostics before
refusing the program.

`record_loop_exit` now calls the existing `loop_count_diag` before
interpreting a count. An invalid command contributes no deposit; the later
canonical check emits its diagnostic once. No new fallback, duplicated
count rule or driver reordering was introduced. This is the same
validation-order hazard that W2b's unset-origin guard had exposed.

After rebuilding, the full `control_flow.sh` passes 140/140 and `taint.sh`
passes 538/538 in both modes. Their existing failing-before checks are the
guard's negative control. The ten valid-count reversions were not rerun
under a claim that the compiler bytes were unchanged.

The corrected gate predicts `NEW=0 FLIPS=6 RETIRED=0 TOTAL=10824`: the new
IDs are already in history, and the six flips must repair the first run's
failures. It passes in 242 seconds, rows **6702565-6713388**:

- 5523 native + 5301 C = 10824 checks; 10811 passing, 13 existing skips.
- Exactly six failing-to-passing flips; no new IDs, retirements, ghosts
  or plan mismatches.
- Liveness returns from the failed run's 266 to 260.
- All 10782 statuses in the last good W2b block, 6680959-6691740, remain
  present and unchanged.

The gate includes `mk test` (237/27/20 units). Formatting, clippy with tests,
totality and the live complexity hook pass without bypass. Final source and
release-compiler hashes match the separately saved guarded positive.

## Separate findings and disposition

**B94 remains an SSA definition-order hole under `--no-taint-cst`.**
The owned nested-if reproducer prints `[dirty]` with that backstop alone.
With the normal front end, both baseline and final compiler report SB5020
at 1:92 in both modes. The one-pass SSA classification issue is separate
from W4's process partition: `bug-ssa-taint-definition-order`.

**B106's old premise was wrong, but a value-split false positive is real.**
DD99 requires replacement even for empty input. Clean and empty value
splits over an argv-filled array still leave boolean taint in the validator;
with `--no-taint`, their runtime results are correctly `clean` and `empty`.
The old `w0/c5` was a pipe, already handled by W2a, not this split case.
The recorded reference work did not settle the split, and its transfer is
untouched here: `bug-value-split-retains-taint`.

**A counted capture jump can leave its caller's loop.** On the baseline
and initial W3 implementation, a capture containing one loop and `break 2`
compiles, exits zero and prints nothing instead of `clean hi`.
`capture_body_into` does not isolate `state.loop_stack`, so the correct
count decoder sees the wrong stack and skips capture cleanup.
`lower_inlined_body` is a candidate sibling, not a measured named-function
claim. These body-entry functions are untouched by W3; the P1
`bug-counted-jump-crosses-capture-boundary` records the source and evidence.

DD121, DD127 and GUIDE describe the implemented transfers without claiming
those separate repairs. The findings index records all W3 dispositions.
The 330-row ledger preserves its original metadata; H265, H269 and H282
are reimplemented, while the END portions remain W5. Totals are 247 ported,
58 reimplemented, 15 dropped, 8 superseded and 2 pending.

Text sources, before/after results, all reversion xtraces, guarded positive
traces, patches, hashes and both exact gate blocks are archived in session
`files/port-w3-probes.tar.gz`. Compiler binaries are excluded; named frozen
compilers remain available for the next unit. No coverage run or reference
tree rebuild was performed.
