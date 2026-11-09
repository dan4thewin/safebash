title: Provisional directory selectors and scratch collections
date: 2026-11-09

**Later the same day:** journal 604 records DG's correction and the
combined provisional immutable-scratch/validated-regex model. The
scratch-retention/no-new-lifetime interpretation below was the AI's
extrapolation, not a separately selected DG rule. DD131 is reconciled
in place; DD132 and `q-glob-semantics`' DD revision inventory cover the
extended trial. This entry preserves the original recording as history.

## From a glob policy question to a branch experiment

DG selected a deliberately coarse file glob: dotfiles need a literal
leading dot, ordering should be simple, no traversal needing several
directory opens for one pattern, and no more than one `*`. He also
required composition in `array f=(.* *)`, `array f=(.*); f+=(*)`, and
`for f in .* * { }`. Errors should follow the established patterns,
probably including suppression.

The AI began reconciling DD39/DD69/DD73 and the existing glob backlog.
The discussion then moved beyond matching: DG proposed an explicit
`glob array pat pat...` command, with assignment as sugar. He accepted
ordinary string operands, asked how other array fillers handled old
contents, and proposed accumulation with no destination change on
failure. He also suggested a regex-pattern counterpart.

DG then suggested a leading-underscore name for a scratch collection,
refining the idea with:

> probably the ephemeral must not already exist...

After discussion of scope, repeated execution and parameter aliases,
DG adopted the package:

> let's adopt glob, rglob, and scratch collections as a provisional
> decision; when implementing we'll use a branch and evaluate the utility
> and ramifications

DD131 records that provisional model. The AI supplied the detailed
working interpretation and consequences below; they are not separately
ratified permanent rules or a claim that the experiment has succeeded.

## The working interface

`glob ARRAY PATTERN...` and `rglob ARRAY DIRECTORY REGEX...` have array
destinations and separate operational statuses. Successful enumeration
succeeds even with no matches. Known patterns are validated statically,
unknown ones at runtime; computation does not evade pattern bounds.

Each glob pattern selects from one directory using at most one star
in its final filename component. Non-wildcard directory prefixes remain
legal; wildcard traversal and additional stars are errors. `rglob`
instead matches basenames with ordinary regexes, with no hidden-file
prefilter. Glob wildcards never select `.`/`..`; `rglob` excludes those
entries from regex selection. Both return normalized absolute paths.

Per-pattern bytewise sorting and source-order concatenation preserve
overlaps as duplicates. No whole-destination sort or traversal engine
is implied. The new selector does not restore DD108's combined
glob/regex array shorthand. Ordinary command arguments do not gain
implicit globbing.

Assignment still replaces, append still appends, and iteration supplies
data rather than a status. Sugar must use fresh intermediate results
where necessary. The exact raw/quoted/computed-string boundary needs an
explicit branch evaluation, not automatic globbing of arbitrary strings
whose contents happen to contain a star.

## Original working interpretation: introduction versus accumulation

The working destination model includes existing array fillers:
`read`, `argv`, `keys`, `match` and `matchall`. Ordinary targets append
the new batch. A `_batch`-style target requests a fresh source binding,
rejecting an already-bound name even if its array is empty.

The AI recommended treating that marker at source introduction, not
as a hidden storage-class flag that a helper inherits through A-E.
Slots already name existing caller collections; fills through them
accumulate. Preserve DD100 identity, signature and non-aliasing rules.
The named producers remain string-array producers; no map merge policy
or scalar `readline` change is implied.

DD86's scope remains: a second source binding site is a redeclaration;
executing one site again in a loop is not. A successful scratch fill
replaces that site's preceding batch. An operation error leaves it
unchanged, so a later failed loop iteration can retain earlier data.
Truly fresh-empty state on every iteration would require a different
lifetime/reset rule. That consequence belongs in the utility evaluation.

Bare `_` already has DD98's discard meaning. It is not a readable
scratch name, and `_batch` inherits none of its taint or unread-variable
exemptions. Scratch is not const, a new block scope or permission to
resolve taint by an ordinary write.

## The current implementation is different

Source inspection at `bad3ff56`, not a new execution probe, found:

| Producer | Current mechanism |
|---|---|
| `argv` / `keys` | `sash_argv_snapshot` / `sash_keys_snapshot` build fresh replacement tables (`sash-rt/src/lib.rs:1099-1145`). |
| Array `read` | `sash_split_into_array` clears at `sash-rt/src/lib.rs:3896`, including for empty input. Native `codegen/dispatch.rs:1391-1423` and C `emit_c.rs:4721-4741` call `sash_read_all` before splitting, so its real read-error path aborts before that clear. |
| `match` / `matchall` | Their out-array functions clear at `sash-rt/src/lib.rs:2588/2660`, before regex compilation/allocation; malformed dynamic regexes can then abort. No match leaves an empty result and false verdict. |

Thus current fillers replace, but their state on an actual error is
not uniformly empty. Neither the old implementation nor simply deleting
the clear calls proves the proposed unchanged-on-error guarantee.

Publication must cover the whole operation, including a failure after
earlier patterns or records produced data and allocation faults visible
to ERR. It is a collection-state guarantee, not rollback of input
consumption, argument effects or other completed work. EOF and a
successfully computed negative regex predicate remain distinct from an
I/O or pattern-compilation error. Existing result kinds must not begin
testing the old length of an accumulating destination.

The glob source inspection still shows singleton recognition in array
and for lowering, hardcoded `"."` directory arguments in native/C, and
runtime open/read errors treated as empty/EOF. The original F13 probe
results are historical and were not rerun here.

## Recording and queued evaluation

The paused seven-file draft is reconciled with DD131 rather than left
claiming that no command handling surface was chosen. DD39/DD69/DD73,
the loop/error DDs and GUIDE distinguish the proposed policy from what
ships. DD60/DD63/DD75/DD99 retain explicitly qualified replacement
contracts, and DD86/DD98/DD100/DD108 describe the relevant boundaries.

`q-glob-semantics` remains the integrated P2 owner, now for a separate-
branch prototype and evaluation. The literal-directory task stays open.
The multi-star task now requires refusal, preserving its original
matcher observation as history. The completed absolute-path task is
corrected to its actual cwd-only scope.

Shared runtime classification/recovery owners and the dynamic-regex
question remain independent; a new status-bearing `rglob` does not
settle every old regex consumer's interface. The existing regex-anchor
bug is a correctness prerequisite, not intended semantics for the
new selector. These dependencies need not be separate commits or wait
for unrelated file-API work.

The trial must measure real usability and migration costs: repeated
snapshots, latest-capture indices, underscore names, new reserved
command names, loop failure state, parameter forwarding, memory use,
taint and converter/quoting behavior. Native/C real-CLI evidence and
before/reversion controls must accompany implementation, with predicted
gate counts derived then. DG reviews adoption, revision or rejection;
there is no automatic landing.

This recording changes Markdown only. No implementation branch,
compiler/runtime/converter/spec/build change, behavior probe, sweep or
coverage run was made. Journal 577 remains the accepted implementation
baseline.
