# 638 - Retiring stale backlog records

2026-11-16. DG asked to drop the Janet tasks, close completed or superseded
items, and offer three lines of work appropriate to begin now. I reconciled
the records against the current decisions, shipped source and implementation
history; this is not implementation of another backlog item.

## Janet retirement

The preceding inventory had 582 task records: 394 terminal and 188
non-terminal, including 32 deferred Janet records. Removed exactly that
32-record cohort, including its parity umbrella and primitive-library plan.
Completed historical Janet records outside that cohort remain.

The removed records are available in the
[pre-cleanup snapshot](https://github.com/dan4thewin/safebash/tree/6e99b1f1845a7159b0ba5dd33b745d13c1bf2403/notes/tasks).
DD104 now points there and freezes its direct primitive-library reference.
Old journal mentions remain historical rather than being rewritten en masse.
The completed C function-capture task loses its obsolete Janet-effort parent;
no surviving non-terminal task depends on that retired cohort.

This does not abandon DD104's independent post-beta oracle or change the
inert IR contract. `janet-babel` still preserves the old backend code;
the pre-cleanup snapshot preserves the later task analysis.

## Eight closeouts

| Record | Disposition | Evidence |
|---|---|---|
| [readline into int](../tasks/bug-readline-into-int-ice.md) | done | Journal 567, the shared target check and `gtype.1`; current native/C emission refuses with located SB4030. |
| [capture-form SSA taint](../tasks/ssa-taint-capture-form-builtins.md) | done | Journal 560 replaces the old capture annotation; current tainted join/echo results reach the SSA backstop, while clean controls compile. |
| [operand-check asymmetry](../tasks/type-check-operand-check-asymmetry.md) | done | `55e0214d` removed the private duplicate and unified its checks; current scalar-len refusals name `strlen` in all three tested positions. |
| [token-renderer dead arms](../tasks/tok-to-text-dead-arms.md) | done | `58cae45b` removed rival renderings; the current exhaustive guard explicitly names the parenthesis interception invariant. |
| [dispatcher dead zone](../tasks/complexity-gate-dispatcher-dead-zone.md) | done | DG's September 16 `cog <= 2` exemption is present in the actual pre-commit query. |
| [clean-pipe while reader](../tasks/bug-validator-taints-clean-pipe-while-reader.md) | superseded | DD127 deliberately taints every kernel read; wider actual buffering has its own owner. |
| [Brush rerun](../tasks/brush-rerun.md) | superseded | DG retired the external suites, with retrieval recorded in journal 621. |
| [rolling complexity cohort](../tasks/cohort-complexity-cog-cc.md) | superseded | DG retained the 3x on-touch gate instead of a cohort burn-down and threshold-tightening program. |

The complexity closeout does not mean there are no remaining hotspots or
that the standing gate is optional. Historical task bodies and measurements
remain, clearly separated from the closeout.

## Kept open

The [empty-splat converter task](../tasks/converter-splat-empty-case.md)
was blocked on a language form that DD84 already supplies: `@{a:~}`.
It is now open implementation work because the converter still emits the
old two-option advice for positional and array splats. No automatic
scaffolding algorithm was selected by this cleanup.

The lockfile task also now acknowledges the completed DD129 API repair,
without dropping its independent fresh-resolution control or claiming that
lock enforcement has shipped.

Mixed residual records and unproved precision/coverage hypotheses remain
open. In particular, a current function-body parse-error control still loses
the specific missing-pipeline-RHS diagnostic, now reporting SB4072 at the
closing brace. That recovery task is not complete. A removed helper name,
accepted policy, or broad passing gate alone is not a closeout.

There are now **550 task records: 402 terminal and 148 non-terminal**.
Those 148 include umbrellas, related delivery owners and deferred questions;
they are not an independent ready-to-run job count.

## Three proposed workstreams

1. **Regex correctness first.** Start with
   [whole-subject anchors and global-search resumption](../tasks/bug-regex-line-anchors.md).
   This is a bounded P1 with a recorded attestation consequence and a settled
   contract, not a new regex-language design.
2. **Usable ERR cleanup under DD135.** Start with
   [NUL predicates and filename tests](../tasks/nul-predicate-and-filename-tests.md),
   then [readonly imports and local proofs](../tasks/err-readonly-inputs-and-nul-proofs.md).
   The clean [mkdtemp value intrinsic](../tasks/mkdtemp-value-intrinsic.md)
   is independent and directly serves the tour's cleanup example.
3. **Reproducible, enforced quality gates.** Begin with
   [tracking both Cargo lockfiles and locked normal builds](../tasks/track-cargo-lockfiles.md).
   Add [required complexity measurement](../tasks/require-complexity-index-for-compiler-commits.md)
   and [checked-input/staged-snapshot agreement](../tasks/q-complexity-gate-without-index.md).
   Their policies are accepted; the private-rustix build repair is already
   delivered and is not a blocker.

These are recommendations to DG, not a selected implementation sequence.
This task does not resume coverage or start work in the separate experiment.

## Evidence boundary

Current compiler controls cover the reader-int refusal, tainted and clean
builtin captures, and the three scalar-len positions in native and C-emission
modes. Two parser controls distinguish the surviving recovery defect. The
token and hook closeouts also use current source and their recorded delivery
evidence; no historical full sweep was rerun.

No compiler, runtime, converter, spec, build rule or hook implementation
changed. The managed status block, both local lockfiles and spec-history
boundary 6947637 are preserved. Session evidence retains the original
inventory, deletion hashes, controls and final scope/accounting checks.
