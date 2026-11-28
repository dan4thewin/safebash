title: Unattended backlog session - naming guards
date: 2026-11-28

# Unattended backlog session - naming guards

DG requested twenty fully determined, low-complexity backlog items, with
decision-dependent or oversized work recorded and deferred rather than
interrupting the batch. DG then required a session branch.
The AI selected the existing owners and created
`unattended-backlog-20261128` from main `4e5090c9`.
Main, the detached reference worktrees and unrelated `notes/dat` are retained.

This first cohort completes five selected owners:
duplicate function definitions, integer-call redefinition, conservative
function names, declaration/loop-binder names and keyword documentation.
It implements DG's existing DD86/DD125 decisions, not new language policy.
The other fifteen selected items remain for the session.

## One identity before analysis

SB5109 runs in the first structural naming batch, before analyses keyed by
function name. The canonical `Program` inventory now supplies definition
spans as well as functions. Every repeated definition names the first
site; a third definition does not move that original site.
Identical bodies, unchanged result kinds and unused definitions still reject.

The template classifier's duplicate-count filter is consequently obsolete
and removed. Unique-function availability/classification machinery remains;
this is not a replacement-semantics or general resolver rewrite.
The integer-redefinition i1-i3 programs now reject before their conflicting
body/type choices can produce output.

Both function-definition readers, every declaration name and word-loop
binder use `expect_identifier`, backed by `ir::is_plain_identifier`.
The old parser predicate delegates instead of maintaining a second algorithm.
Malformed word names receive SB4014 at the name, with the exact rejected
spelling and `[A-Za-z_][A-Za-z0-9_]*`. Existing non-word and reserved-keyword
diagnostics retain their paths; attest targets do not change.

The AI extracted the existing for-word reader to keep the touched parser
within the live complexity gate. Header behavior is unchanged in this
cohort; required `in` and nonempty source lists are the next selected owner.
No runtime, backend or source-name normalization change was needed.

## Converter and keyword explanations

The converter annotates actual Bash function definitions with malformed
names using normal MANUAL guidance. It keeps names and calls unchanged,
including distinct `read-file` and `read_file`, rather than merging identities.
Both normal and annotated output are covered. Literal/comment text does not
trigger the hint. The current converter has no separate canonical-output mode.

DD125 and the `KEYWORDS` explanation now describe position-independent
structural keyword tokens and the parser's eight-token name admission.
Command-like reserved words such as `return` still use `Word`.
The documentation correction does not alter keyword verdicts.

## Evidence

The final new namespace is `fnuniq.1`-`.15`, `fnident.1`-`.5`,
`declident.1`-`.6` and native-only `convname.1`-`.3`.
Seven old definition-in-force contracts retire:
`nonvoid.8`, `.19`, `.25`-`.29`. The first focused positive run exposed the
first two; their xtraces showed the intended early duplicate refusal, and
they were migrated at fresh IDs rather than weakening the guard.

Across the four affected spec files, the frozen original tools fail exactly
46 assertions: all behavior-changing additions/migrations. The nine new
preservation assertions pass before the change. The final repaired files
pass in both compiler modes and native-only converter mode.

Three independent final-tree reversions establish the mechanisms:

| Reversion | Expected failures |
| --- | --- |
| Disable the early duplicate guard | All 15 `fnuniq` checks per mode |
| Disable the shared identifier guard | `fnident.1`-`.3` and `declident.1`-`.4` per mode |
| Disable converter naming hints | `convname.1`-`.2` |

Every observed set matches. Twelve additional original/fixed native/C
controls preserve valid-name own/global/argument taint refusals.
All stubs are restored, with exact positive source/compiler/converter/runtime
and lockfile hashes checked before and after the full gate.

```sh
NEW=55 FLIPS=0 RETIRED=14 TOTAL=13467 timeout 900 mk -p 1 gate
```

The gate passes: rows 7611051-7624517, native 6,851 plus C 6,616;
13,454 passing and 13 existing skips. All 13,412 retained baseline IDs
keep their statuses. New IDs, retirements, plans and liveness (246) match.
The sweep takes 339 seconds; Rust unit phases remain 240/27/20.

Private evidence under `files/batch20-20261128/` retains frozen original
tools, standalone TSV/TAP/xtrace records, reversion evidence, hashes and
the gate log. No coverage run or dependency refresh was performed.
