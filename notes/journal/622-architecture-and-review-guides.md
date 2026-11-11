# 622 - Rebuilding the architecture and review guides

**Date:** 2026-11-11

DG asked how stale and useful `docs/GUIDE.md` and `docs/REVIEW.md` were.
The assessment found useful recent architecture explanations inside a
mixed-age GUIDE, and a REVIEW still based largely on the original scaffold.
DG approved the recommendation: retain GUIDE's architecture role in a much
shorter rewrite, and replace REVIEW with a current review checklist.

The AI rewrote both documents. This is documentation work, not a new
language decision, a compiler audit or the public language specification.

## What was worth keeping

GUIDE's binding identities, checked-type handoff, read-boundary taint,
process-local verification, captures, parent readers, handlers and syscall
layer described important current machinery. The rewrite organizes those
around the shipped driver and ownership boundaries.

The surrounding older material undermined that account: it attributed
Bash coercions to the compiler, promised signing/tamper properties, named
retired reader forms, described obsolete IR/SSA shapes and called the
heap a bump allocator. Its test instructions still described failures
whose causes had already been removed.

REVIEW's deleted CST/lowering/rules pipeline, missing source paths and
file-size budgets were not a usable map. More importantly, it directed
validation review at a backend API rather than the real CLI.

## The replacement

GUIDE now distinguishes safebash, sash and sash-convert; explains the
actual driver sequence and the separate runtime workspace; and links to
source instead of copying enum definitions and runtime API inventories.
It preserves the meaningful distinctions among lexical identity, SSA
versions and physical storage, and among integer values, stdout captures
and process statuses.

The driver trace was checked against source at `801c75e1`:
structural validation precedes command resolution; bindings and semantic
checking precede lowering; lowering consumes the actual `CheckedTypes`
and builds SSA once. Signals, terminal checks and dead-variable checks
follow. Native and C emission run the SSA taint backstop, while
`--emit-ir` and `--manifest` leave through inspection routes without that
last gate. The IR output is the pre-binding AST as inert data, not SSA or
an executable Janet backend.

The guide does not claim that `compile_to_object` validates source, that
buffers imply effect-free bodies, or that children inherit a precise
fork-time taint snapshot. DD127's recorded precision limits remain
explicit. The no-libc runtime is described honestly as Rust plus vendored
C and assembly, and component AArch64 syscall support is not presented as
full host/target delivery.

REVIEW is now an evidence-oriented checklist: trace the actual CLI path,
preserve binding/type/aliasing contracts, examine both taint layers and
executable roots, account for process/descriptor/capture ownership, keep
result and failure channels distinct, and follow both emitters into the
runtime. Source specs, internal invariant tests, negative controls and
coverage each have their own evidentiary role.

Neither document duplicates the current test counts, gate-accounting
recipes or accepted-but-unimplemented policy catalogue. AGENTS owns the
workflow; DDs and tasks retain policy and delivery status. The legacy-suite
archive pointer is retained. The public specification and licensing-notice
rollout remain separate work.

## Verification and boundaries

The rewrite was checked against the current driver, SSA construction and
emission entry points, shared capture classifier, runtime/build sources
and the relevant DD implementation records. This is source inspection,
not a new execution measurement or a claim that the recorded limitations
were repaired.

Documentation validation covers local links and heading fragments, code
fences, prose/scope review and Git whitespace checks. Normal commit and
push hooks remain in force. No compiler, runtime, converter, spec, build,
manifest or lockfile change belongs to this task; no new spec sweep or
coverage measurement is claimed. Journal 619 remains the latest
implementation-gate evidence, and status's generated gate block is
preserved.
