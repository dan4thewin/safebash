# 642 - Plan 9 regex subject boundaries

DG chose to keep the Plan 9 engine and proceed with its repair, then
selected fixing default anchors now and deferring multiline syntax.
The AI implemented that choice without an engine migration, dependency
change or new regex flag. DD135 was already complete at `e17196ea`;
this is the separate P1 regex task.

## One subject, a moving search cursor

The matcher formerly treated `^`/`$` as line anchors. An attest using
`^[a-z]{1,32}$` could bless the entire `abc\nadmin=true` or
`admin=true\nabc` value because one line matched. The BOL start optimization
also skipped to later lines. All three sites now enforce subject boundaries;
`$` has no exception before a trailing newline.

The two global runtime wrappers had a second fault: `matchall` passed a suffix as
the subject, and substitution copied a fresh suffix on every iteration.
Each resume therefore made `^` true again. The existing `regexec9` API
already separates the original subject from an input search cursor in
`Resub[0].s.sp`; both loops now use it. Captures retain absolute offsets.
Replacement copies only the span between the cursor and the next match,
then advances to the absolute match end.

Replacement also uses an aligned pointer array for its ten Resub slots,
allocates the terminated subject once, and appends the remaining counted
bytes once. No new FFI type, public ABI or duplicated matcher is introduced.
The other three runtime consumers already passed complete subjects and
need no wrapper change.

## Coupled empty and NUL boundaries

Before implementation, executable controls exposed additional faults in
those same loops. Empty-subject substitution never called the matcher;
global empty matches omitted the terminal position. Conversely, zero-width
searches could advance beyond an interior NUL and match or alter its suffix.

DD63 already requires every match, one-byte progress after an empty match,
and the first-NUL stopping point. Existing `matchall` already includes the
terminal position after a nonempty match. Applying those contracts
consistently gives:

- empty-subject `sub`/`gsub` with `^$` produces the replacement;
- globally replacing the empty pattern on `ab` with `.` gives `.a.b.`;
- `a|$` on `a` has two matches, so global replacement by `X` gives `XX`;
- iteration stops at the first NUL; substitution retains that byte and
  every following byte unchanged.

These were AI-derived consequences of the existing iteration contract,
reported to DG before the code change, not separately invented flags or
a claim of Bash/Perl equivalence. Plan 9 leftmost-longest selection,
capture layout, bytewise operation and the newline exclusions of dot and
negated classes remain. Anchors still do not prove a counted subject
NUL-free: validation requiring that property also needs `0?`.

## Before controls and reversions

The `39.` history high-water was 56. Eighteen dual-mode checks,
39.57-39.74, raise `regex.sh` from 114 to 132 checks per mode. They cover
both attest bypasses, all consumer routes, grouped anchors, resumed
captures/unmatched spans, empty matches and the retained NUL suffix.
NUL assertions compare strings inside sash rather than losing bytes in
Bash command substitution.

All eighteen final fixtures fail on the original compiler/runtime in
both modes, while all 114 older cases pass. Every new program compiles
and executes with status 0 and no diagnostics; the assertion sees the
wrong result. The first 39.59 draft instead triggered SB5086 through a
literal-only test. It was corrected to use variable subjects and both
before runs were repeated. The invalid draft control is not evidence.

All 132 cases pass after the repair in each backend. Seven independent
mechanism reversions then produce exactly these failures, identical in
native and C:

| Reversed mechanism | Failing `39.` IDs |
|---|---|
| BOL instruction and start optimization | 58, 59, 60, 62, 63, 68, 69 |
| Strict EOL instruction | 57, 59, 61, 62, 63 |
| Original subject in `matchall` | 66, 67, 69 |
| Original subject in replacement | 64, 65, 68 |
| First-NUL bound in `matchall` | 73, 74 |
| First-NUL bound in replacement | 72, 74 |
| Empty/terminal replacement search | 70, 71, 74 |

Every new check is pinned. Each red assertion again follows a successful,
diagnostic-free execution, not a malformed fixture. The BOL paths are
reverted together: this proves the semantic repair, not independent
necessity of both guards. The existing C-engine harness passes all 152
checks; no rival unit suite was added.

The first reversion runner stopped at the native spec's expected exit 1.
Its completed TSV and xtrace were retained and checked, then C mode was
run; no spec rerun was needed. All fourteen reversion traces match their
ordered TSV IDs. Every core stub was restored, and the sources and artifacts
then matched the core snapshot, `positive.sha256`. The later helper changes
have a separate final snapshot.
A copied compiler alone would not freeze the native runtime: linking uses
the archive in the build checkout. The original before-controls were run
while that actual archive was still original.

## Accepted core gate

`NEW=36 FLIPS=0 RETIRED=0 TOTAL=11621 timeout 1200 mk -p 1 gate` passes
at rows 6993646-7005266: 5922 native and 5699 C, 11608 passing and thirteen
existing C skips. No failures, flips, retirements, ghosts or plan mismatches.
All 11585 statuses in the saved prior block 6982061-6993645 are retained
unchanged. Liveness reports 251 historical differences; the sweep takes
267 seconds. Rust unit/doc checks pass with 239 compiler, 27 converter and
20 expansion tests. No coverage run or dependency change is included.

## The retained standalone helper

The final consumer check found the same two global-loop defects in
`sash-rt/csrc/sash-regex.c`. Although its header identifies the retired
Janet use, `mk build` still builds this executable. The AI aligned its
capture and substitution loops rather than deleting the target or
reviving the backend. Both keep the original subject and an independent
cursor; substitution also searches empty/terminal positions and appends
the unmatched remainder once.

Three dual-mode checks, 39.75-39.77, raise the final plan to 135.
They run the actual helper through compiled sash programs; `od` renders
NUL-separated captures without losing bytes in the Bash harness.
With a frozen pre-repair helper first on PATH and the repaired compiler/
runtime unchanged, exactly those three checks fail in each mode.
All three programs compile and run with status 0 and no diagnostics.
With the repaired helper, all 135 checks pass in each mode.

The first draft quoted an absolute command name and failed with SB5096
before execution. A later capture expectation had one extra NUL separator:
six fields have five separators. Both mistakes were corrected before
the accepted before/after runs, named `helper-{before,after}-verified-*`.
The discarded runs are not evidence of the repair.

Three isolated one-line reversions were compiled with the normal helper
command and selected through private PATH directories. Restoring suffix
subjects in captures makes only 39.75 fail; doing so in replacement makes
only 39.76 fail; excluding the terminal position makes only 39.77 fail.
Both backends give those exact sets, with successful diagnostic-free
executions and all 135 trace IDs matching each TSV. The working helper
and sources were never replaced by these variants.

The first helper-reversion trace extraction used the old byte offset
after `setup` replaced an expired weekday directory. Its complete new
trace was recovered and checked against the run's TSV, helper path and
timestamp; the spec was not rerun. Later captures held the old files open
and recognized replacement by inode. This was an evidence-extraction
mistake, not a harness or compiler failure.

## Final gate, 2026-11-17

`NEW=6 FLIPS=0 RETIRED=0 TOTAL=11627 timeout 1200 mk -p 1 gate` passes
at rows 7005267-7016893: 5925 native and 5702 C, 11614 passing and thirteen
existing C skips. No failures, flips, retirements, ghosts or plan mismatches.
The six first-ever IDs are the helper checks; the earlier 36 already have
history. Across the task, 21 source checks add 42 mode IDs.

Independent row-block comparisons preserve every status from both the
11621-check core gate and the 11585-check pre-task baseline. Liveness is
251 and the sweep takes 267 seconds. `final.sha256` matches the four
changed implementation files, spec, compiler, helper, runtime archive and
both local locks before and after the gate. The locks also match the
original pre-task snapshot, not merely the final positive.

## Records

DD39/DD63 now state the strict anchors and shared cursor/end contract.
DD39's byte-identical matcher claim is struck; `regaux.c` and the original
vendor reference remain unchanged. The concise manual no longer advertises
the repaired anchor bug and still states the NUL/newline limits. The draft
outline and dependent documentation/trial pointers agree. No new
architecture requires a GUIDE rewrite.

The P1 is closed. `regex-multiline-mode` records the deferred public
syntax decision without choosing a spelling. Literal-regex caching,
better static diagnostics, DD131/DD132, general dynamic-pattern failure
handling and platform work remain separate.

Session evidence is in `files/plan9-anchors-20261116.NEBI2k/`:
`before-final-*`, `after-*`, seven core reversion sets with predicted and
actual results, `positive.sha256` and the implementation patch;
`helper-{before,after}-verified-*`, three isolated helper variants and
their six reversion runs; `final.sha256`, both gate logs and exact
row-block comparisons. Earlier invalid or incorrect fixtures remain
explicitly superseded by the verified ones.
