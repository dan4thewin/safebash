---
title: "Resolved: cleanup bodies cannot define functions"
status: done
priority: P4
kind: question
depends: []
gates:
  - "DD56 and DD128 state program-top-level-only definitions"
  - "SB5071 rejects a fn in END; definition-only support and 56.94-56.100 are removed"
---

DG resolved this on 2026-11-04: retain SB5071 uniformly for EXIT/ERR and
remove support solely for the old END exception (journal 566).

The premise of this question was wrong. DD86 already allowed definitions
only as direct elements of the program's top-level statement list.
END's acceptance was a validation defect, not permission to be reconsidered
only after F3 had completed its downstream implementation.

SB5071 now rejects direct END definitions too. `Program::function_defs`
remains the shared inventory but no longer includes END. Definition scans,
scope enumeration and taint definition checks no longer treat cleanup as
another definition root; the redundant lazy registration remains deleted.

Seven exception-only specs are retired. Six typing, alias and signal
checks retain their actual claims with helpers moved to program scope.
Handler calls, shared globals and complete body checking remain necessary
for END and its DD128 replacements. EXIT/ERR implementation remains W5's
work; this placement question is closed.
