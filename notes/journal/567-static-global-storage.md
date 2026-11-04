# 567 - Actual globals before generated ERR

2026-11-04. DG chose actual process-global storage and ERR as a generated
function. His priority is a model that is easy to reason about, not keeping
shell-like programs' variables in registers. This settles the architecture:
ordinary terminal calls, no jump back into the generated main frame.

I implemented the storage prerequisite on a1ff78bd. The compiler still
accepts END, not EXIT/ERR. Generated handlers, terminal routing and the strict
adjacent-resolution rule remain subsequent work; this commit does not
expose a partially checked new handler surface.

## One backing store for each global

`bindings::global_names` exposes the existing lexical inventory. Lowering
combines it with the checker's actual type map and records `SsaProgram::globals`.
Collections have a distinct physical kind, including int-element collections:
their storage is a table handle, not an integer cell. No second name or type
inference pass was added.

Native emits writable data symbols. `ScalarSlot` distinguishes a local
Cranelift variable from a component of a global data object; the existing
load/store consumers use that distinction. C emits file-scope static
objects and excludes those names from main's local declarations. Every
version of a global addresses the same backing store. Function locals,
compiler temporaries and source-function inlining are unchanged.

The checked-map invariant exposed a real omission: implicit bulk-read
targets were collections but had no entry in `TypeEnv.vars`. Existing
`56.85` failed with "every source global has a checked type". Scalar reader
targets needed registration too. The checker now supplies these entries;
lowering does not invent a String default for missing names.

The scalar-reader check also rejects an int target with SB4030 at the
command. Previously the native backend hit "expected string var, got int",
while C emission reached a compiler failure. `gtype.1` pins the located
diagnostic and distinguishing message.

## Meaningful zeros without startup allocation

B13 was directly coupled: merely moving a null collection pointer from a
local to a global would preserve the crash. Each global collection instead
starts with its own 32-byte zero header. Missing reads, missing deletion,
length, clearing and iteration treat it as empty. The first mutation
allocates ordinary backing and installs it in that header. Headers are not
shared singletons; growth frees backing, not the static header.

Two integration checks mattered:

- A pointer relocation in `define_zeroinit` did not initialize the pointer
  in the linked NOBITS object. Headers and integer zeros may use BSS;
  pointer-containing objects now use actual stored bytes plus relocations.
- `from_raw_parts(NULL, 0)` is invalid even though no element is read.
  Joining a zero-entry table now uses an actual empty slice. Removing that
  guard is not a reliable red test: undefined behavior can still print the
  expected output. A distinct-exit sentinel proves the new zero-header
  branch is reached by the actual source checks.

Empty strings revealed the other representation boundary. A collection cell
uses a null value pointer to tag an integer, so a purported empty string
with `(NULL, 0)` turns into integer zero when stored in a collection. Native
string globals now point at the already-interned empty literal.

Real `readline` EOF exposed the same problem in both backends: copying the
result into an array and joining it printed `[0]`. The backends now store
an actual empty-string pointer while deriving the EOF verdict from the raw
returned pointer. EOF remains false; an empty line remains true. The runtime
ABI is unchanged.

## Retagging is not copying

B97 was not solved merely by moving ordinary variables to globals. Native
statement attestation created new locals for the target version and copied
the source only on success. END could read that future-only copy after an
earlier abort, before the copy ran.

The retag now aliases the source's storage, matching the caught-attest path.
`gstate.1` preserves `safe` when a capture fails before its assignment
completes. `.2` preserves `changed` after a parent reader completes but its
producer fails. Both explicitly use `:!` and exit 3. They pin values, not
handler-entry cleanliness: B110's taint half is still W5.

The independent controls separate the two mechanisms. Reverting the retag
alias fails exactly these two native cases. Disabling global metadata while
retaining the alias leaves both cases passing in both modes.

## Source evidence and controls

The new cases are `gstate.1`-`.2`, `gzero.1`-`.8` and `gtype.1`.
`gzero` covers untouched arrays/maps, missing cells/deletes, first mutation
and growth, int cells, distinct A/B arguments, empty splitting, fork
isolation, initial empty strings and real EOF.

The final before-control uses the frozen a1ff78bd compiler and runtime.
All eleven new cases fail natively; eight fail in C. C's two `gstate` cases
and initial-empty-string case are preservation controls. Positive
signals/subscript/types runs pass 73/141/471 checks per mode.

Two early fixtures were invalid evidence: top-level `$#` was SB5024, and
`argv a` was not empty because it includes argv0. The accepted fixtures use
quoted `$(argc)` and a clean empty-scalar split. The invalid results are
retained but not counted as negative controls.

Seven reversions and one reachability control were run independently.
Each runtime control used the frozen positive compiler. EOF's two
backend-local changes were reverted together; they do not share an emission
path.

| Control | Native red set | C red set |
|---|---|---|
| Copy statement-attest targets | gstate.1-.2 | none |
| Omit global metadata | gzero.1-.7 | gzero.1-.6 |
| Omit lazy table backing | gzero.2-.4, .6 | same |
| Omit empty-table get guard | gzero.1-.4, .6 | same |
| Omit empty-table clear guard | gzero.5 | same |
| Omit empty-table delete guard | gzero.2 | same |
| Exit 97 at a zero-header join | gzero.1, .4 | same |
| Store raw EOF pointers | gzero.8 | same |

All predicted sets match; no old check fails in these runs. The positive
source, compiler and runtime identities were restored and verified before
the full gate. The allocation-collector extraction made for the complexity
gate preserves each block's instruction-then-phi insertion order.

## Gate and remaining work

`NEW=22 FLIPS=0 RETIRED=0 TOTAL=10910 timeout 900 mk -p 1 gate` passes
on the first sweep: 10897 pass and 13 existing skips, native 5566/C 5344,
rows 6767773-6778682, 245 seconds. Unit counts are 238/27/20. No ghosts,
plan mismatches, flips or retirements; liveness is 260. The exact prior
block, 6756885-6767772, retains all 10888 statuses unchanged.

The normal formatting, clippy, totality and live complexity gates pass
without bypass. There was no coverage run. Source and evidence are retained
in the session's `port-dd128-globals-probes.tar.gz`; frozen compiler/runtime
identities are separate. No reference hunk is completed solely by this
prerequisite, so the remaining W5 ledger rows stay pending.

B13 closes for globals, and B97 closes. B14 still owns per-call local
initialization and lifetime. Next are strict adjacent resolution and the
complete checked EXIT/ERR surface, including generated callbacks, fatal
dispatch, actual-entry taint contracts and protected signal checkpoints.
