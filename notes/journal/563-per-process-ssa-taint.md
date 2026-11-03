# 563 - Per-process SSA taint checks

2026-11-03. DG authorized W4 on November 2, starting from `5f6fee32`.
I implemented the remaining
process-local SSA backstop, retaining DD127's independent function summaries
and the validator/lowering barriers already landed in F1/F2/W1. END entry
remains W5; the separate array-timing and definition-order faults are not
silently included in this unit.

## What remained

The front end already isolates child name effects, and lowering already
saves/restores versions around a forked child. The backstop still classified
every SSA definition into one map before checking every use. A child's
array cleanup could therefore clean a parent or sibling read in the
analysis, and a child's private taint could falsely reject a clean read.

The reference H167 supplied a separate `Pass` and recursive `check_process`.
H158's shared process traversal was already mostly present from F2; only
its sorted `process_blocks` helper remained. This is a per-process verifier,
not the rejected caller-context AST walker.

Each process now classifies its definitions and checks its uses against its
own view. It then checks every child found by the existing `spawned_by`
authority, handing each an independent copy of the completed parent view.
The same-process closure excludes forked descendants; recursive calls check
those descendants in turn. Both background jobs and lifted function/argument
pipeline stages use the existing CFG representation. A final parent-side
reader stays in the parent's closure.

Ascending block order is preserved. This is still one classification pass,
not a fixpoint, and an inherited view still includes parent definitions
after the fork. Process isolation does not establish per-use array versions
or precise fork-time state.

## The reference driver alone was insufficient

The initial port fixed four missing refusals, but all six positive controls
still failed. Their saved xtraces showed the SSA backstop, not runtime
misbehavior or a different diagnostic.

The scalar example was the W1b-1 stage-write reproducer:

```
str x=clean
fn fill { x=$(/bin/echo dirty) }
- echo hi | fill
echo "$x"
```

An owned real-CLI SSA dump established the mechanism. Main block 0 defined
`x_1` but read `x_2`. That version was assigned in unreachable template
blocks 1-2, while the actual child wrote `x_3` in blocks 3-5. Executable
reachability was `{0,3,4,5}`. Merely lowering the definition had changed the
parent's current-version map, and the reference main view included every
non-child block, including the template.

Two coupled corrections were necessary:

- Keep the templates for lowering diagnostics and metadata, but save and
  restore current versions around `lower_fn_template`. The existing
  snapshot deliberately does not rewind version-allocation counters.
- Root the verifier's main same-process closure at entry and END, rather
  than every non-child block. This excludes unbound templates and retains
  implicit abort entry into END. Actual inlined calls remain executable
  and checked. Codegen already excludes unreachable templates.

The reverse defect is a missing refusal: assign tainted `x`, define a
function that assigns `x=safe`, never call it, then read `x`. The old
backstop accepted the read even though the program printed `dirty`.
`procflow.14` pins that definition-time version leak.

No template deletion, runtime change, new language rule or bypass switch
was needed.

## Specs and independent controls

`taint_switch.sh` grows from 48 to 62 checks per mode. `procflow` had no
current or historical IDs before this change.

| IDs | Claim |
|---|---|
| 1-3 | Scalar/array parent reads remain clean after child writes |
| 4, 6 | Background and pipeline siblings have independent views |
| 5 | A grandchild's writes do not taint either parent |
| 7-8 | Child replacement and attest cannot clean the parent's array |
| 9 | A child inherits existing parent taint |
| 10 | A grandchild's own tainted read is checked |
| 11 | A final parent-side reader still taints its target |
| 12 | Lifted command-argument stages are child processes too |
| 13 | One sibling's cleanup cannot clean another sibling |
| 14 | A never-called function cannot clean the parent's scalar version |

The first six run with both taint layers enabled; the remaining eight use
`--no-taint-cst` to isolate the backstop. Positive assertions require the
exact output and empty compiler/runtime stderr. Negative assertions name
both the relevant tainted value and the SSA backstop.

The authoritative `before-final` runs, using the frozen W3 compiler, fail
eleven new checks per mode: `.1`-`.8` and `.12`-`.14`. The first six are
false refusals; the other five compile and execute without diagnostics.
`.9`-`.11` already pass. Earlier draft fixtures incorrectly passed a scalar
argument to zero-argument `fill` functions and hit SB5043; those records are
retained separately, not counted as taint evidence. The corrected fixtures
pass `dirty` to the program because `argv` reads program arguments.

The repaired positive passes all 62 checks in both modes. Seven independent
reversions were then built and run serially, each restored before the next:

| Reversion | Red IDs in each mode |
|---|---|
| Include unbound templates in the main view | `procflow.2`-`.6` |
| Do not restore definition-time versions | `procflow.14` |
| Let process closures cross forks | `procflow.2`-`.8`, `.12`, `.13` |
| Stop recursion after immediate children | `procflow.10` |
| Give each child an empty inherited view | `procflow.9`, `.13` |
| Share one mutable view among children | `procflow.4`, `.6`, `.13` |
| Reverse block order | `procflow.10`, `.11`; `sw.9`, `.14`-`.16`, `.26`, `.27`; `bkstp.16` |

The last prediction was too narrow: it named only `.10`. The native trace
showed all nine programs compiled with exit zero and no diagnostic, so the
extra failures were real missing refusals, not harness noise. I retained
that result and checked C against the observed nine-ID set; it matched.
This pins preserved ordering, not a repair of the known forward-phi hole.

All source hashes and compiler bytes matched the saved positive after
restoring the controls.

## The first full gate exposed a template-dependent spec

The first gate predicted `NEW=28 FLIPS=0 RETIRED=0 TOTAL=10852`. Its
243-second sweep, rows **6713389-6724240**, ran exactly that total and
recorded exactly 28 first-ever IDs. Only `bindscope.39` failed, in both
modes. Both programs were still correctly refused.

The xtrace showed `b_t_2` and `t_2`, while the spec required version 1.
Its diagnostic-name collision depended on unreachable templates. Merely
changing the expected suffix would have lost that collision: the first
binding's live version no longer equalled the other binding's base name.

`bindscope.40` replaces `.39` at a fresh historical ID. It names the second
local `t_2`, making `__a_b_t_2` both the first live value's formatted SSA
token and the second binding's qualified base. The assertion still requires
both correctly restored source names and forbids leaked qualified names.
This is a preservation adaptation, not a weaker diagnostic assertion.

All 158 collection-parameter checks pass with this fixture on both the W3
baseline and the W4 positive, in both backends. An eighth independent
reversion restores names only after joining the SSA suffix. It turns
exactly `bindscope.38` and `.40` red in each mode; `.40` actually calls the
first variable `t_2`, proving the identity collision. That stub is removed,
and the production source/compiler still match the original positive.

The corrected gate predicts `NEW=2 FLIPS=0 RETIRED=2 TOTAL=10852`: the
28 process results are already recorded, and the two old `.39` registry
entries are explicitly retired in favor of `.40`. Their history remains.
The compiler implementation has not changed since the first sweep.

It passes in 242 seconds, rows **6724241-6735092**: 5537 native + 5315 C
= 10852 checks, 10839 passing and 13 existing skips. There are exactly two
first-ever IDs and the two predicted retirements, zero flips, ghosts or
plan mismatches, and liveness returns to 260. An independent comparison
against the last-good W3 block confirms all 10822 retained statuses are
unchanged; only `native:bindscope.39` and `c:bindscope.39` are absent.

The gate includes `mk test` (237/27/20 units). Formatting, clippy with tests,
totality and the live complexity hook pass without bypass. Final source,
both spec files and compiler hashes match their saved positive identities.

## Remaining boundaries and ledger

W4 completes B18's remaining backstop work. END remains an explicit root,
but its entry state across aborts, exits and signals is still W5. The
completed-parent view and highest-version array lookup retain the separately
filed timing fault; the single-pass classifier retains B94's ordering fault.
W3's clean-split and counted-capture defects are unchanged.

DD127, GUIDE and the findings index describe the actual boundaries. The
array-timing task now says per-process completed view rather than a global
map. The 330-row ledger preserves all original metadata: H158's remaining
helper is complete, H167 is reimplemented with the executable-root repair.
Totals are 247 ported, 59 reimplemented, 15 dropped, 7 superseded and
2 pending. W5's mixed spec rows remain pending.

Evidence is archived in session `files/port-w4-probes.tar.gz`: before/draft/
positive results, complete xtraces, the measured SSA, reversion diffs,
source hashes and exact gate row blocks. All 136 members (7,725,052
uncompressed bytes), including 38 complete xtraces, were compared with
their originals; every trace's ordered check IDs matched its result rows.
Frozen compiler binaries remain separate. No coverage run or
frozen-reference-tree rebuild was performed.
