title: Why the tour uses NUL records and scratch
date: 2026-11-18

# Why the tour uses NUL records and scratch

DG found the new read and scratch examples unconvincing: features unfamiliar
from Bash need to explain why they exist, not merely show their syntax.
He suggested piping from `find` to motivate NUL records and asked where
scratch makes a program easier to reason about. The AI revised five programs
in the authored tour and regenerated its GitHub edition.

## Problems before mechanisms

**Filenames are not lines.** Two `find -print0` pipelines gather files from
separate directories into one ordinary array, then copy them together.
One filename contains a space; the other contains a literal newline.
Both survive as individual elements and command arguments. The explanation
connects NUL-only `read` to `find -print0` and `xargs -0`: Unix filenames
cannot contain NUL, but whitespace is valid filename data.

This replaces the arbitrary `first`/`second` accumulation example. The two
reads still demonstrate accumulation, now with a reason to gather batches.
Each acquisition has an adjacent attest checking for regular files, rather
than rejecting unusual names. The slide moves after the taint and line-reader
introductions so its validation and pipeline do not precede their explanation.
The stdin and in-memory forms, assignment's replacement behavior, and
`readline`'s separate line-record role remain explicit.

**A frozen rollout plan.** A mutable candidate list supplies an immutable
snapshot, then changes for a later rollout. The original plan does not
silently acquire the changes. Its companion refusal catches an attempted
append to the frozen plan: neither later code nor a helper can modify that
instance.

**One log batch per service.** A loop reports directories containing two,
one and zero log files. `glob _logs` gives counts 2, 1, 0, without a manual
clear; an ordinary accumulating target would give 2, 3, 3. This example
now follows the selector introduction. The explanation retains the important
limit: freshness follows execution of the construction site, not block scope.

**Reject a record, not the job.** A numeric import reads 12, invalid text and
30. Caught scratch conversion totals the valid records and counts the bad
one, without a fabricated replacement or a jump solely to resolve failed
taint. Each read is fresh; the successful conversion supplies an immutable
number. Only the total and rejection count are mutable running state.

## Evidence and scope

The existing `mk -p 1 tour-doc` builds the native helpers and runs all
**71 programs**, with **15 intended failures** and zero outcome mismatches.
Independent assertions check each revised output and the single SB5103
diagnostic on the frozen-plan append. GFM parsing confirms the other
**66 programs and their recorded outputs are unchanged**.

An additional real-CLI run inspects the actual archive directory: it contains
exactly `annual report.txt` and `two\nlines.txt`, where `\n` denotes the
filename's newline byte. A second control changes the directory example's
scratch target to an ordinary target and obtains the documented 2, 3, 3
counts. Both controls use normal taint checking and clean up their temporary
files.

The starting commit is `4624102b` on `glob-scratch-trial`. This is a
documentation revision, not a compiler change, new language decision or
adoption of DD131/DD132 on main. The helper sources, presentation
configuration, compiler/runtime artifacts and local lockfiles are unchanged.
The product spec history still ends at row 7271250; no new product sweep,
C-backend measurement or coverage run was needed.

Only the authored deck, generated README, this journal and current status
change. Private evidence is in the session's `tour-motivation-20261118`
directory.
