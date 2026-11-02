# 560 - Taint at the read boundary

2026-11-02. DG continued W2, then questioned whether an external could
write to its parent's stdout regardless of its own redirect. That exposed
the wrong trust boundary in the uncommitted routing draft. DG proposed the
replacement and selected its scope: kernel reads introduce taint;
compiler-owned buffer copies introduce none; use the existing buffer paths
first. I implemented that W2a unit. W2b's shared-output attest restriction
is not included.

## Why descriptor routing was discarded

The owned probe captured a function that printed `internal`, then ran an
external with `>/dev/null`. Opening `/proc/PPID/fd/1` did not change the
capture: sash temporarily redirects the parent's fd 1 too. But the parent
still held saved descriptors for its memfd capture. Reopening one of those
let the external overwrite the captured value with `external`, in both
native and C, with exit status zero and no compiler diagnostic.

Thus `/dev/null` was not an isolation proof. Ordinary pathnames were
already suspect because they can alias `/dev/stdout`; special-casing one
pathname did not fix the mechanism. The source/channel/witness routing
draft was removed, rather than patched with a runtime alias guard or a
process-isolation scheme.

The exact source now gets located SB5020 at its final use in both modes.
The previous actual-write evidence is retained; the old executable was
not rerun to rediscover the result.

## One read boundary, existing buffers

Every fd read taints, including a pipe fed only by `echo` or `write`.
Every kernel-backed capture taints, including an anonymous body containing
only a builtin redirect. A compiler-owned buffer introduces no taint, but
does not declassify data copied into it: the value still has to pass its
ordinary use check, through attest or explicit `:!`. Environment and
program-argument inputs remain sources.

This does not require rejecting every function containing an external.
The existing capture machinery permits an outer buffered function to take
an inner external capture, attest it, then echo the result. The runtime's
transparent inner capture frame suspends the outer buffer while the kernel
capture runs. Its final outer copy is not a kernel read.

Conversely, direct externals, redirects, pipelines and a recursive purity
visit currently choose the general memfd path. A streaming wrapper's own
read/attest does not attest the later kernel read of that wrapper's output.
Those are intentional consequences of DG's existing-buffers choice, not
claims that the old clean-streaming/redirected examples were fixed.
`q-buffer-capture-eligibility` records the separate optimization question.

Integer and intrinsic values are not stdout captures. An integer function
can return a clean integer while printing externally; the print still makes
an enclosing body capture kernel-backed. The classifier keeps those two
effects separate.

## Implementation

- `output_taint.rs` is a small source classifier, not another name-flow
  walker. It distinguishes value calls from real captures. Its body table
  includes every definition of a function name; integer-return recognition
  requires all definitions to return integers, so one cannot hide another's
  stdout. This can conservatively reject a redefined name whose particular
  active body would buffer.
- `capture_purity.rs` moves from `transform` to the compiler root so source
  checking and lowering use the same predicate. Its eligibility rules and
  existing unit tests are unchanged.
- The validator passes that source context through its existing read checks.
  Every fd target is tainted directly. The old producer/runs-source scans
  and clean-pipe exceptions are gone. `read SRC ARR` remains a value split,
  with its existing source-use checks and pending B106 precision question.
- SSA `ReadLine` and `ReadArray` always taint. `CaptureEnd` derives taint
  from its actual `buffered` choice. There is no separate capture `tainted`
  annotation, `clean_pipe`, `ExecFns`, or analysis object in `VersionState`.
  The lexical reader binding now holds only its fd.
- W1's independent global/slot summaries are unchanged. The only incidental
  extraction is `attest_validation`, separating binder/clear/message
  selection from the attest driver to satisfy the live complexity gate.
  Runtime executable code is unchanged; its module-location comment is
  corrected.

## Specs and controls

`outflow.1`-`.14` bring taint.sh from 469 to 483. They cover direct and
transitive function output, redirected and pipeline captures, builtin and
function pipe readers, clean shared buffered wrappers, internally attested
input, and the integer-value/stdout distinction. Rejections require
front-end SB5020 and exclude an SSA backstop failure.

`sw.26`-`.28` bring taint_switch.sh from 45 to 48. They disable the front-end
taint pass and require the actual SSA backstop to reject a builtin scalar
pipe, a builtin array pipe and a kernel-backed function capture. Existing
`bkstp.6` supplies the clean-buffer counterpart.

The full fixture census found **63 old cases**, identical across modes:

| file | affected cases |
|---|---:|
| taint | 9 |
| errors | 16 |
| misc | 4 |
| pipe_functions | 10 |
| capture_cond, comments | 1 each |
| control_flow | 7 |
| functions | 3 |
| optional_args, signals | 1 each |
| types | 2 |
| wordsplit | 8 |

Their xtraces all showed SB5020 at the newly tainted use. Runtime, grammar,
status and lowering assertions are retained using an adjacent attest,
localized `:!`, or the existing `SASH_CFLAGS=--no-taint` mechanism where
the original capture expression must reach lowering. `dd82.4` was itself
an assertion of clean user-function capture; its unchanged source now
requires located SB5020. An initial edit hit `cap.19` instead of `cap.20`;
it was corrected before the final affected-file runs.

All thirteen affected spec files pass in both modes. Against the saved
W1b-2 compiler, the final taint file has exactly nine red cases per mode:
`dd82.4` and `outflow.1`-`.6`, `.9`, `.14`. The other two rejecting
outflow cases, `.7`/`.8`, were already rejected before; the first prediction
had incorrectly counted `.7` as new. The switch file adds exactly three
red cases per mode, `sw.26`-`.28`.

Five independent reversions establish the mechanisms and pin every new
case, including the controls that already passed before:

| reversion | red per mode | focused witnesses |
|---|---:|---|
| frontend kernel captures classified clean | 58 | outflow.1-.4, .9, .14; dd82.4 |
| frontend skips fd-reader targets | 20 | outflow.5-.8 |
| SSA capture buffer/kernel result inverted | 4 | sw.28; bkstp.6, .9, .12 |
| SSA fd readers classified clean | 4 | sw.26, .27, .9, .15 |
| frontend treats every capture/value as tainted | 26 | outflow.10-.13 |

The reverted binaries differed from the saved positive. All stubs were
restored; source hashes and the rebuilt compiler match the saved positive
byte-for-byte. Measurements before the gate used standalone TSVs, not
spec.db.

## Gate and records

`NEW=34 FLIPS=0 RETIRED=0 TOTAL=10714 mk -p 1 gate` passes on its first
run. The sweep took 241 seconds, rows **6670245-6680958**: native 5468,
C 5246, **10701 passing plus 13 skipped**. Exactly 34 first-ever ids,
zero flips, retirements, ghosts or plan mismatches; liveness is 260.
Units are 237/27/20. Formatting, clippy with tests, totality and staged live
complexity pass without bypass. Source hashes and the rebuilt compiler
still match the saved positive after the gate.

DD24 and DD127 are rewritten in place; GUIDE and AGENTS agree. The findings
index closes B33's classification bug and explicitly marks the old B35,
B114 and e5 expectations as superseded by DG's rule. Forty-one ledger rows
are updated, preserving all 330 rows' original hunk metadata. Its W2-only
rows are resolved; H282 remains pending solely for W3's ordinary loop
deposits. Shared-output provenance is not claimed.

## Remaining

W2b must establish shared-output provenance and the outside-attest
diagnostic under the read boundary, not restore the discarded pathname
analysis. Ordinary loop edges, per-process SSA checking, END entry and the
already-recorded name/array precision issues remain their separate units.
No coverage work or reference-branch modification was done.

Evidence is in session `files/port-w2a-probes.tar.gz`: 306 members, including
the before/after and reversion TSVs, gate log, owned descriptor probes and
complete xtraces for the thirteen affected files. Every archived member
was compared with its source. The two compiler baselines are excluded from
the archive and retained in `/tmp/safebash-w2.gFBe6Co0` for the next unit.
