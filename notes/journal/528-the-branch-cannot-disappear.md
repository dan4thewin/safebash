# 528 - The branch cannot disappear

2026-10-19. DG continued coverage from `522a9d5`: 82 cold compiler
production lines. This batch selected all eleven remaining cold lines in
`transform/control_flow.rs`, read the complete functions and their
producers, then separated source gaps, correct API declines and obsolete
fallbacks. No language decision was needed.

## The conversion hypothesis did not fail

The arithmetic-conversion path restored versions differently from the
ordinary caught-attest path. That suggested a lost post-if value, but it
was a hypothesis, not a finding. Literal and dynamic `$(arg 1)` controls
all passed before the edits in both native and C: success, a modified
target, failure and no-else conversion under `--no-taint`. The same dynamic
controls pass afterwards. No source conversion defect was demonstrated.

Both predicates now use the existing shared caught-attest merger. It
constructs the full condition branch directly instead of constructing a
partial branch and conditionally patching its false edge later. The
same-function "maybe this is not a Branch" guards disappear. Exhaustive
predicate dispatch also removes a second decline the classifier cannot
produce: arithmetic conversion always has one scalar source.

The no-else source arm was a genuine coverage gap, not a reason to relax
SB5089. Two new checks in `taint_switch.sh`'s existing `--no-taint` section
pin a modified successful target and the failed target's default zero.

## A selected construct cannot quietly disappear

Two direct API controls failed before the repair, specifically because
they did not panic:

- A malformed selected `if attest` returned the incoming block, discarding
  its condition and branches.
- An empty dispatch silently passed through an obsolete early-success
  path, bypassing its existing incoming-path invariant.

The first now refuses explicitly. Its comment names the unsafe consequence,
not just the earlier gate. The second no longer has a compatibility bypass;
it reaches the same shared "fewer than two incoming paths" invariant as
other malformed dispatch ASTs.

Producer enumeration matters here. The parser's only real `Case` producer
seeds a non-default arm; a `matches!(StmtKind::Case(_))` classifier is not a
second producer, despite its index classification. No accepted source
program demonstrated either malformed-IR failure.

Two argv-slice arms retain their correct answer: a substitution with zero
or multiple statements, or a suppressed/redirected statement wrapper, is
not the exact simple argv slice. A direct API matrix asserts `None` for
each shape rather than merely executing the lines.

## One false-edge scan, not two

Dispatch no-match rewiring previously visited the named check blocks,
allocated a second block-id list, and scanned it with exclusions to find
multi-pattern sub-blocks. One mutable scan now redirects exactly the false
edges targeting this dispatch's unique merge block. A structural API test
pins matching false edges, unchanged true edges, unrelated branches,
gotos and returns.

The new `cflow.1` source check exercises first, middle and last alternatives
in a three-pattern arm, then the no-default fallthrough. The actual new
spec files, not merely candidate snippets, warmed both original source
gaps before the simplification. Their trace outcomes were checked because
`cov-batch --spec` does not itself fail on a failing suite.

An initial fixture inspected `$(status)` after dispatch; DD#55 makes that
SB5012. It was a fixture error, not a compiler bug. An ordinary no-else
caught-attest probe similarly hit the intentional SB5089; its supported
test context is `--no-taint`.

Four API units replace one rival. The retired live-iteration unit repeated
concrete source assertions: `cf.f1` for word iteration and `75.7` for map
keys. The compiler now registers 219 units, up from 216.

## Fresh complete-scope measurement

The selected file closes **11 -> 0**; compiler production cold lines fall
**82 -> 71 / 30726**. The denominator falls by 22 production lines. No
cold-file regressions; binding resolution and both emitters remain zero
cold. All **42** measured source blobs are current.

Raw cargo-llvm-cov: **99.23% line / 99.57% function / 94.88% branch**,
including inline test scaffolding. Exactly **9118 CLI profiles across all
32 slots** distinguish source execution from API reachability:

| Site | CLI | Union |
|------|----:|------:|
| Selected malformed attest | 0 | 1 |
| Argv substitution body decline | 0 | 2 |
| Argv statement-wrapper decline | 0 | 2 |
| Dispatch incoming-path invariant | 0 | 2 |
| Conversion's shared merger call | 20 | 20 |
| No-else merge | 26 | 26 |
| No-match false-edge rewrite | 34 | 35 |

The instrumented sweep matches its prediction exactly:
**9255 = 9242 passing + 13 existing C skips**, native **4730**, C **4525**.
All **9249** baseline statuses at **6211981-6221229** are preserved, with
exactly the six intended and first-ever mode IDs: `ifcv.1`, `ifcv.2` and
`cflow.1` in both modes. No unexpected additions, retirements or previous-row
flips; liveness remains **260**. The sweep block is
**6221230-6230484**. Independent source plans and both global ID collision
checks agree, including `coverage.sh`'s two-check variable-id expansion.

Saved the CLI/union exports, merged profiles and coverage database before
ordinary restoration. The remaining cold files are expression handlers
(17), statement handlers (8), type checking (14) and validation (32).

## Ordinary publication gates

The first `mk cov-reset` removed the instrumented artifacts, but its
recursive default-parallel `mk build` hit the installed Go runner's
previously filed mutex/channel deadlock. It failed before the ordinary
tests or sweep, not in a compiler assertion. Journal 511 and the existing
`go-mk-test-deadlock` task document the same runner failure; the task now
includes the reset/build trigger. Serial rebuilding and the final ordinary
gates completed with `mk -p 1 build` followed by `mk -p 1 test`. That is the
established serialized gate, not a repair or root-cause claim about the
runner.

The full ordinary units pass: **219 sash + 27 convert + 20 expand**.
The ordinary sweep repeats exactly **9255 = 9242 passing + 13 existing
C skips**. Its block is **6230485-6239739**, with zero first-ever IDs.
All 9249 baseline statuses and exactly six intended additions remain;
no retirements or previous-row flips, liveness 260.

Default and configured native/C compiler/runtime canaries preserve both
successful and failed no-else conversion, a middle-pattern match and
no-match continuation. Neither compiler nor program emits a profile;
existing target profiles remain unchanged. Source plans, both ID collision
checks and fresh 42-file provenance pass. No architecture, language, ABI or
runtime change required a GUIDE or DD update.

Final staged fmt, clippy, totality and live touched-function complexity
gates pass without bypass. The named probe executable is removed; retained
evidence stays in the session workspace.
