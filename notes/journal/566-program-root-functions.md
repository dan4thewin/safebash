# 566 - One function-definition root

2026-11-04. DG noticed that the typing prerequisite's END-defined functions
contradicted SB5071. He directed retaining the rule uniformly for EXIT/ERR,
discarding support solely for the old exception, correcting DD56, and
continuing DD128 implementation.

DD86 had already settled the broad rule: a definition belongs only as a
direct element of the program's top-level statement list. I had treated
existing acceptance as permission, completed its downstream implementation
in F3, and filed rejection as a new design question. That was a misreading,
not an unresolved language choice. Sharing global variables does not make
a cleanup body another definition root.

## What remains, and what is removed

The placement check now distinguishes the program statement list from
cleanup: it rejects functions below a program-level statement and every
function inside END, including a direct one. Its shared statement walk
includes captures. DD128 carries the same rule into EXIT and ERR.

`Program::function_defs` remains the canonical inventory, now main-only.
Removing the shared helper would only recreate private copies of its useful
work. Binding, typing, function classes, summaries and registration retain
the helper. The redundant lazy integer-return registration stays deleted.

The obsolete definition traversal is removed from command resolution,
predicate-definition collection, undeclared-read names, scope enumeration
and taint definition checks. Two private recursive collectors and the
scope-body selector disappear. The structural validator's earlier
syntactic-name collector still visits misplaced declarations for
diagnostics; it does not grant placement permission.

Main and cleanup remain two executable bodies during the migration.
Handler commands, calls and shared global collection facts still require
complete traversal. The checked type map remains lowering's sole source
of types. Nothing here replaces that repair with another inference walk.

## Specs retain the valid claims

The seven `56.94`-`56.100` specs existed solely to support END-defined
functions and are retired. `fnroot.1` instead requires a located SB5071
with the instruction to move the helper to program top level.

Six other specs retain their actual claims with their helper definitions
moved out of END. Changed programs take fresh IDs above the database
high-water marks:

| Retired ID | Replacement | Retained claim |
|---|---|---|
| typeflow.8 | typeflow.25 | Integer capture typing in cleanup |
| typeflow.18 | typeflow.26 | Integer function rejected as a cleanup predicate |
| typeflow.21 | typeflow.27 | Integer-array slot inferred from a cleanup call |
| typeflow.22 | typeflow.28 | Forwarded integer-map slot from cleanup |
| noalias.13 | noalias.17 | Global/slot non-aliasing at a cleanup call |
| sig518.6 | sig518.18 | Pending signal does not re-enter cleanup through its helper or loop |

On the frozen `f3868337` compiler, `fnroot.1` is the sole failure in each
mode's 71 signal checks; all 470 typing and 158 collection-parameter
checks pass. After the correction, those files and all 245 function
checks pass in both modes. Disabling only END's placement guard turns
exactly `fnroot.1` red again in each mode. The stub is restored and the
positive source/compiler hashes match before the full gate.

## Gate and records

The predicted gate passes on its first sweep:

```
NEW=14 FLIPS=0 RETIRED=26 TOTAL=10888 timeout 900 mk -p 1 gate
```

Rows 6756885-6767772 contain 5555 native and 5333 C checks: 10875 pass,
13 existing skips, no failures, flips, ghosts or plan mismatches.
Exactly fourteen IDs are first-ever and twenty-six retire; liveness is
260. The sweep takes 242 seconds; units are 238/27/20. An independent
comparison preserves all 10874 retained baseline statuses and matches the
exact new/retired ID sets. All 330 ledger metadata rows are unchanged.
No coverage or runtime change.

Final review removes a dangling doc comment left above the deleted
definition collector and reflows the registration comment. Those are the
only Rust changes after the sweep; the implementation is unchanged.
The final unit/doc run passes all 238 compiler units, and `final.sha256`
records the rebuilt final compiler and sources.

Evidence is retained in the session archive
`files/port-dd128-placement-probes.tar.gz`: before/after/reversion TAP,
TSV and complete xtraces, the exact gate comparison, hashes and final diff.

DD56's exception is replaced in place, DD81/DD55/DD128 and GUIDE agree,
the former question is closed, and the findings and walker-sweep records
distinguish definition inventory from executable-body traversal. H068 is
reimplemented as a program-only inventory; the other generic F3 repairs
remain. Historical journals retain what was actually done at the time.

EXIT/ERR grammar, routing, strict adjacent taint, explicit ERR handling,
signal protection and fatal transfer remain W5 work. The next architectural
choice is how a fatal runtime helper reaches ERR: today's cleanup is a
label inside generated main, while arithmetic and allocation faults call
`sash_exit` directly. The existing setjmp/longjmp machinery is currently
used only for Plan 9 regex error recovery, not as a program-handler mechanism.
