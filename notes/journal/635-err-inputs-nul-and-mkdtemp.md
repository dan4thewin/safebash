# 635 - Readonly ERR inputs, NUL proofs and mkdtemp

DG asked to record the ERR-input discussion as a decision, update the
dependent rules and specification outline, and add implementation to the
backlog. DG also asked why completed code changes were still uncommitted.
This task records policy and accounts for that work; it implements none
of the new policy.

## Decision

[DD135](../decisions/DD135-err-readonly-inputs-nul-and-mkdtemp.md) records:

- ERR receives readonly access to inherited logical bindings, including
  through same-process helpers, captures and collection aliases. New
  ERR-owned working state and fresh private locals retain ordinary rules.
  This is not a new scope, a global const rewrite or a public readonly
  operation.
- `0?` is true for NUL in counted contents, false for empty strings. DG
  explicitly selected **"Contains a NUL byte; false for an empty string"**.
  The trailing storage terminator does not count. The NUL-free predicate
  is `! 0?`; `attest s 0?` would instead validate NUL presence.
- Filename predicates return false on any NUL-bearing pathname before
  lookup. Either invalid operand makes `-nt`/`-ot`/`-ef` false; treating
  NUL as a missing file would leave a false-positive `-nt` path.
- Existing ERR byte facts remain the fast path. Otherwise, a use-local
  query must establish the right predicate outcome for the exact argv
  value on every reaching path. Readonly supplies stability, not the
  proof. Taint remains independent and runtime argv guards remain.
- Zero-argument `$(mkdtemp)` creates a new owner-only directory under
  fixed `/tmp`, retries collisions and returns its generated pathname
  directly, clean and NUL-free. It does not read `TMPDIR` or capture
  stdout. Ordinary checked creation and capture-condition handling apply;
  handled recoverable failure gives empty text plus failure, never a
  previous pathname. An inner `-` discards the real creation status;
  fatal faults remain fatal.

The AI kept implementation ownership in three focused tasks:
[predicates/file tests](../tasks/nul-predicate-and-filename-tests.md),
[readonly ERR imports/local proofs](../tasks/err-readonly-inputs-and-nul-proofs.md),
and [mkdtemp](../tasks/mkdtemp-value-intrinsic.md). The proof task depends
on the predicate contract it will trust; the constructor is independent of
the larger existing file-I/O backlog. This is staging/ownership, not a new
language choice or authorization to implement.

The dependent current rules were reconciled in place: DD12/87/99 for counted
strings and C boundaries; DD86/100/128 for readonly access; DD93 for `0?`;
DD24/127 for source classification; DD55/70/88/111/130 for direct values and
operational handling. DD131/DD132 remain provisional, with the scratch
task explicitly distinguishing their experiment from the accepted ERR
restriction. The list-only outline includes the accepted behavior without
compiler mechanisms or claims of shipment.

The import implementation still needs a real identity-based classification.
`HandlerEntry.inputs` currently contains every global, including ERR-only
working bindings. The scalar const checker is not a complete collection-
mutation checker. The new task records both cautions rather than promising
that readonly is just a flag on either existing table.

## Existing code: two finished repairs, not ERR experiments

The actual tracked implementation diff contains eight Rust files and four
spec files. It matches the work recorded in journals 633 and 634:

| Work | Actual changes | Origin |
|---|---|---|
| Indexed updates evaluate once | Seven compiler files retain a compound cell operation in IR, lower the key once, save the old cell before RHS effects, preserve semantic reads and advance inert IR to schema 3; `subscript.sh`, `misc.sh` and `cli.sh` pin it | The tour's counter/increment discussion, followed by DG's requests to continue; [journal 633](633-indexed-updates-evaluate-once.md) |
| Long `readline` records | Only `sash_read_line` in the runtime changes: retain the stack fast path, grow for longer records and end only at newline/EOF; `wordsplit.sh` adds the regressions | DG's request for safebash versions of the tour helpers; [journal 634](634-tour-helpers-in-safebash.md) |

The three untracked `.sb` helper sources are also from that explicit
side-by-side port request. Their original script counterparts were retained.
The tour, outline revisions, symmetric-lookup policy and DD134 records are
the other pre-existing review work. No readonly/NUL-predicate/file-test/
mkdtemp implementation was hidden among these changes.

### Why they were left uncommitted

The original outline request explicitly said **"create a draft outline
(do not commit)"**. DG later requested a branch and a commit of that
original version, followed by revisions. That snapshot is `2ef86c2a`
on `spec-outline`.

The AI subsequently carried the outline's draft-only treatment over to
the tour work and the two completed bug repairs. The repair/port replies
reported that everything remained uncommitted, but reporting the state
did not make it a DG instruction. There was no corresponding explicit
request to leave those completed compiler/runtime fixes uncommitted.
Extending that boundary to the fixes was the AI's mistake, not a required
part of either repair.

This recording does not silently stage, commit, revert or reorganize that
work in place of answering DG's question. The new policy records also
remain in the working tree.

## Evidence and preservation boundary

The existing unary filename probe printed `directory` for both `/tmp`
and counted `/tmp\x00not-a-path`; the guarded ERR helper still received
SB5101. That earlier native evidence is in
`files/tour-cleanup-directory-proof-20261114.json` in the session.
The binary filename-test trap is source-inspection evidence, not a newly
executed native/C matrix.

Journals 633/634 retain the implementation gates and negative/reversion
evidence. Their latest sweep ends at spec.db rowid **6924875**. No build,
spec sweep or coverage measurement was needed or run for this documentation-
only task.

The task's private `files/err-input-policy-20261114/` contains the original
tracked diff and hashes/modes of 1,552 tracked/untracked files, along with
HEAD, the index tree, the managed status block and that spec-history boundary.
Review compares this recording against that baseline, not just against
HEAD, so inherited changes cannot disappear inside the documentation diff.

The recording-only review covers exactly 25 Markdown paths. All render,
all 110 local links resolve (42 newly introduced), existing record metadata
is unchanged, and only outline topics 7/11/12/13/20/21 change. The draft
remains one nested ordered list. All 1,532 untargeted baseline files,
including the compiler/runtime/spec changes and original tour files, retain
their bytes and modes. HEAD, branch, index tree, the generated status block
and spec-history boundary are unchanged. `recording.patch` and
`validation.json` preserve the scoped diff and checks.
