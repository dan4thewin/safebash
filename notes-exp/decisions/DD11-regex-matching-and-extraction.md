---
title: "DD#11 - Regex matching and owned extraction"
status: accepted
---

DG authorized the bounded `match`/`matchall` unit after byte-string helpers.
The modified ECMAScript `std::regex` engine was already selected by DD#1.
Native DD#63 guides the operation shapes but does not automatically impose
its pragmatic engine restrictions on this independent target.

## Choices and their reasons

| Choice | Authority / reason | Observable consequence |
|---|---|---|
| Host standard-library ECMAScript | DG's pragmatic dependency/portability starting point, DD#1 | Not Plan 9 grammar or alternative ordering; no extra regex dependency |
| Full counted subjects | DG explicitly chose byte-transparent subjects | NUL is ordinary subject data, not a stopping point |
| Standard repeated iteration | DG chose C++ iteration; reuse avoids bespoke progression logic | Original anchor/boundary context and nonempty retry after empty matches |
| Atomic out-array replacement | DG delegated to simplicity; AI chose local collection plus swap | Exception preserves the previous target; normal miss still clears it |
| Explicit literal-NUL pattern rejection | Small portability guard after a direct standard-library probe failed | Error names the problem and directs to the ordinary `\x00` regex escape |

The NUL and iteration policies are deliberate target differences, not
claims that native behavior is a bug. Native's first-NUL boundary was kept
pragmatically under YAGNI; the target no longer has that engine limitation.
Do not treat engine selection alone as having silently settled these
separate policies.

## Shared operations and API

Use ordinary string values/views and compiled `std::regex` values:

- `match(subject, regex)` returns a boolean search result.
- `match_text(subject, regex)` returns an owned whole match, or empty on miss.
- `match(subject, regex, array)` replaces it with `[whole, g1..gN]`.
- `matchall(subject, regex, array)` flattens each match's groups, or its
  whole match when there are no groups.

The AI chose separate scalar `match_text` naming and bool/out-array
overloads as reversible C++ API details; C++ cannot overload solely on
the desired return type. No wrapper with implicit string/bool conversions
is introduced.

All captures are independent owned strings in a dense standard-map array.
Nonparticipating and participating-empty groups keep their empty slots.
Retain every group reported by the selected engine, not an obsolete
nine-group cap. Empty matches are success; the scalar form necessarily
conflates their empty value with no-match, while the boolean form distinguishes.

Build into a local array and swap on completion. This is simpler than
clear-first plus an input snapshot, preserves the target on exceptions,
and safely consumes a subject borrowed from that target. Completed no-match
replaces the old target with empty. Allocation errors escape; standard
regex errors use the existing `CommandError(2)` / `regex: ` renderer.
No helper adds a checkpoint or suppresses a body failure.

## Iteration examples

For subject `ab`, pattern `^`, native fresh-suffix iteration emits three
empty spans, while standard C++ iteration emits one at the original
beginning. This pair was executed through both the real CLI and C++ runtime.

Under the same ECMAScript engine, subject `a`, pattern `(|a)`, standard
iteration yields `["", "a", ""]`. Immediate byte-skipping after the first
empty match would yield only `["", ""]`. This compares iteration policies;
do not mislabel it as the native engine's result for that alternative.
Standard iteration emits the final empty boundary once and terminates.

## Pattern admission and failure continuation

This host's direct `std::regex` range constructor accepts a literal-NUL
pattern but fails to match it; `\x00` and matching across a subject NUL
work. Reject literal zero bytes in pattern input with an explicit status-2
diagnostic rather than ship a silent wrong answer or a rewriting adapter.
The ordinary regex escape remains supported, and subjects stay fully counted.
An externally constructed regex does not retain an inspectable source
pattern; use `compile_regex` for this target's admission policy.

Native `regcomp9` failure aborts status 2; a condition or `||` cannot resume
that in-process call. C++ may catch its standard runtime exception. Neither
policy is an ordinary false match, and no native recoverability is claimed.
DG separately requested the native backlog task
[`reject-literal-nul-patterns-and-source`](../../notes/tasks/reject-literal-nul-patterns-and-source.md);
the broader raw-source ban still needs its native decision.

## Scope

The eleventh pair extracts harmless option records into captures and a map,
converts values, and uses ordinary functions/branches/traversal.
`sub`/`gsub` templates, bulk reading/splitting, and other runtime families
remain separate work. There is still no IR reader/emitter or producer
implementation, and no native code change is included.
