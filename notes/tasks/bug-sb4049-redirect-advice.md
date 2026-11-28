title: SB4049 recommends redirect targets that DD119 rejects
status: done
priority: P3
kind: bug
depends: []
gates:
  - "SB4049's target examples and capture workaround satisfy DD119's nonempty-target rule"
  - "the advised capture-then-guarded-redirect program compiles and writes the expected file in both backends"
  - "direct capture targets still receive SB4049 and an unguarded variable still receives SB4030"
  - "the advice assertion fails before the wording repair or under its reversion"

# SB4049 recommends redirect targets that DD119 rejects

## Resolution, 2026-11-28

SB4049 now shows nonempty defaults and require dispositions, and advises
binding the capture before redirecting through `${v:?missing filename}`
after any required attest. The target restrictions are unchanged.
Existing `14.19` now checks the advice and executes the suggested repair;
it fails on the original compiler and when the wording is reverted.
Both backends pass the ordinary gate.
[Journal 684](../journal/684-unattended-diagnostics-and-c-literals.md)
records the proof.

## Original report

The redirect-index ICE task already recorded this independent wording defect.
While closing that repair on the DD131/DD132 trial, source inspection still
finds it in `sash/src/parse/parser.rs::reject_nonscalar_redirect_target`
(lines 4139-4143 before this task's creation).

SB4049 lists bare `$x` and `$a[i]` as supported targets and recommends:
"Assign it to a variable first, then redirect to `$v`."
DD119 instead requires a provably nonempty target; a variable or cell needs
a nonempty default, a require disposition, or a nonempty surrounding word.
Following the advice literally produces SB4030 rather than a working program.

Correct the guidance, not the target rules. For example, the capture
workaround can use `${v:?missing filename}`. Preserve the existing direct
capture-target ban and the separately allowed guarded cell-index captures.
This is not a request for broader dynamic filename syntax.

The earlier task's execution evidence is historical. This filing verifies
the current message and DD119 contract by inspection; no fresh execution
or wording repair is claimed. Reuse an existing message assertion where
possible rather than adding a rival diagnostic test.
