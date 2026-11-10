title: Make path-specific file failures catchable and suppressible
status: open
priority: P2
kind: bug
depends: [runtime-failure-classification]
gates:
  - "sash-owned path-specific file failures identified by the DD130 inventory have real recoverable handling; missing handling support is not relabeled fatal"
  - "a command's recoverable redirect-open failure is implicitly checked, caught or suppressed by its owning context in both backends"
  - "a failed open skips the command and later redirects, restores scoped descriptors before continuation and never sends output to the original destination"
  - "fatal causes remain fatal under conditions, negation and suppression; handled file errors never invent a successful default value"
  - "actual failing paths, ordered earlier side effects, diagnostics, captures and saved handler status remain correct"
  - "function result kinds and no-reach into helpers/operand captures remain intact; ERR analysis agrees with the actual handling"
  - "DD88 direct file-read capture-conditions consume real read status: empty completed reads succeed, handled errors bind this attempt's bytes, and unhandled reads remain checked before assignment"
  - "focused native/C specs fail on the original behavior or targeted reversions, and the predicted ordinary gate passes"

## Decision

DG placed path-specific file errors in the recoverable class (DD130,
2026-11-06; journal 581). A failed destination need not prevent useful
work elsewhere. The rule is not limited to output redirection: use the
classification inventory to account for the other sash-owned file APIs
and their actual failure/result contracts.

The concrete current mismatch is:

```sh
- /bin/true > missing/out
echo continued
```

With `missing/` absent, this currently aborts with status 1 instead of
continuing. The equivalent suppressed pipeline stage continues silently.
Both native results were recorded at `a7787b24` in
`q-setup-failure-suppressibility`; no repair has been implemented.

## Required handling

For an ordinary path-open failure, the unchecked form still aborts, a
legal condition handles failure, and `-` discards it. No body runs when
its required redirect failed. Restore descriptor mappings before any
continuation; never fall back to writing on the original stdout/stderr.
Stop the redirect list at the first failure, reporting that path, without
pretending to undo files already created or truncated by earlier redirects.

The classification prerequisite matters: `open` can fail for reasons other
than the selected path. Suppression must not discard a classified fatal
fault merely because it came from the same API as a recoverable failure.

Preserve actual value/status contracts for read, capture and other file
operations. If a form has no way to expose its recoverable failure, design
that handling consistently with the existing types before changing it;
do not fabricate empty success or turn a void/int result into a status.
Raise any genuine language-surface fork with DG.

## Direct file-read capture-conditions

DG selected **"yes to capture-condition"** on 2026-11-10 (DD88,
journal 610). This task owns that extension; `dollar-lt-file-policy` is
closed as a decision, not a second implementation owner.

Admit the direct `SlurpFile` RHS in the existing untyped capture-condition
contexts: branches, loops, negation and logical-list consumers. It binds
text and consumes the read's operational status, not a truth value invented
for assignment. Completion is success, including an empty file.

On handled recoverable failure, bind this attempt's bytes, possibly empty
or partial, never the target's old value. An open failure supplies empty
data **and failure**; an error after progress can supply partial data.
Keep existing byte/newline behavior. Ordinary unhandled slurps remain checked
before assignment, and fatal causes remain fatal. A scratch target in the
separate DD131 trial gets its new instance, not a previous one.

Preserve the first-class in-process file read; do not lower the new form to
an external `cat` or introduce a reader command. Share classification and
result handling across native/C and ERR analysis. Release the read's owned
descriptor before recoverable continuation and preserve actual error causes
and progress. No runtime ABI is selected by this policy.

The outer handler does not catch filename-evaluation failures, nested
captures or called-body failures. Taint, adjacency, ownership, declaration
restrictions, source-order evaluation and ordinary status-latch rules remain.
Concatenated RHSs, arbitrary function/multi-statement captures and other
ineligible values stay excluded. There is no new `$(- <file)` spelling or
permission for `- text=$(<file)`.

Acceptance must distinguish the independent mechanisms:

- Pin direct-file acceptance and real success/failure in the existing
  consuming contexts, including `$?` and negation, with normal taint checks.
  Empty-file success must not be mistaken for an error or EOF falsehood.
- Exercise missing/unreadable paths and a recoverable read error after
  actual progress. Trace the intended file/failed syscall before claiming
  partial-data coverage; an empty open failure does not prove that path.
  Use a target with prior data to prove that failure does not reuse it.
- Retain unhandled aborts, fatal-fault controls, byte/newline behavior,
  filename-operand no-reach, rejected RHS shapes and ordinary external/
  `getenv` capture-conditions. Do not use a blanket SB5101 exemption in ERR.
- Run focused native/C before-controls and independent reversions for
  classification, status consumption and failure-data publication. Choose
  fresh spec ids from history and predict the full gate only after the
  actual fixture inventory is known.

The initial native evidence is in `dollar-lt-file-policy`. It demonstrates
the current SB4025 refusal and unchecked abort, with a working external
capture control; it is not proof of C behavior or mid-read publication.

## Related accepted work

DD131 (2026-11-09) provisionally supplies a concrete directory-selector
surface: `glob`/`rglob` have operational statuses, succeed on zero
matches, and preserve an ordinary existing destination on operation
error. A scratch construction leaves a fresh empty instance, not the
preceding batch or a partially produced one.
`q-glob-semantics` owns their separate-branch trial and the related
accumulation/immutable-scratch model. DD132 adds validated runtime regex
operands, not an exemption from directory failures. Coordinate
open/read/getcwd and partial-progress paths with this task; no
assignment/loop status is invented.
The trial's all-or-nothing collection update does not roll back consumed
input or prior argument/file effects, and fatal causes remain fatal.
External capture failures still preserve this attempt's partial stdout
under DD88; do not extend array atomicity to every scalar producer.
This is queued work, not a claim that the current glob silently-empty
failure paths have been repaired.

This does not extend a prefix into a helper body or argument capture.
DD86 separately settles attached-redirect timing: arguments first in both
standalone commands and stages (2026-11-07; journal 585). Coordinate with
`q-stage-capture-redirect-order`, which owns that pending repair: a failed
open must not undo or pretend to skip completed argument effects. The
direct builtin stderr optimization is independently owned by
`bug-builtin-stderr-backup-restore`; retain its actual-effect proof rather
than suppressing SB5101 around unused machinery.
