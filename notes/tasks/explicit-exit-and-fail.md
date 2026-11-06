title: Separate orderly exit from explicit failure termination
status: open
priority: P2
kind: feature
depends: []
gates:
  - "ordinary main-process exit selects EXIT for every i64 status, defaulting to 0; fail selects ERR for every i64 status, defaulting to 1, including fail 0"
  - "both operations terminate rather than return a catchable status; native and C preserve argument evaluation and the integer-sink rules"
  - "full status reaches the selected handler and survives ordinary cleanup; only final process termination narrows it"
  - "exit in either handler terminates directly; fail in EXIT enters ERR once and fail in ERR terminates directly; children run neither handler"
  - "source and SSA checks include all explicit exits in EXIT entry requirements and recognize fail as a terminal path without clearing taint"
  - "missing-handler, signal, capture, ERR-failure and child-ownership contracts remain intact"
  - "focused native/C specs, meaningful before-controls and independent reversions precede the predicted ordinary unit/spec gate"

## Decision and authorization

DG authorized implementation on 2026-11-06 after discussing orderly
completion with a nonzero status, and explicitly chose to allow `fail 0`.
DD128 section 2 is authoritative; journal 575 records the decision.
`exit [N]` requests orderly termination, whereas `fail [N]` requests failure
cleanup. The operation, not the number, selects the handler. `fail` does
not add an automatic failure message or a message-string argument.

Each builtin accepts zero or one integer argument. Keep DD89's full i64
domain, no string coercion and no range clamp. A failure during argument
evaluation still follows the ordinary failure rules. An `if`, `||` or `-`
cannot turn a reached terminal operation into a returning command.
Preserve exit's existing process-placement restrictions for fail.

The handler blocks remain blocks. There is no source-level callback
signature, arbitrary argument interface, handler return value or new runtime
dispatch architecture. Existing main/EXIT/ERR progression already supplies
the required once-only behavior.

## Implementation starting points

The live runtime already has separate `sash_finish` and `sash_abort` entries.
The old numeric choice is in `sash_exit`. Audit its internal callers too:
an operational failure must not accidentally become orderly shutdown.
Both backends already import the failure entry.

Integrate the new builtin through classification, resolution, integer
typing, backend emission, divergence and signal planning. Source
`validate::taint_call` and SSA `TerminalFlow::explicit_exit` currently
filter EXIT entries with `exit_may_be_zero`; the new rule removes that
numeric distinction rather than inventing a new incoming-data analysis.
Helpers, collection-slot mappings and failed-attest paths remain included.

An ERR-bound fail retains ERR's conservative input and operational-failure
rules. Neither operation clears unresolved input. Explicit termination in
functions and anonymous captures must also count as non-returning paths
in the return-kind work.

## Evidence and migration

Distinguish intentional fixture migration from new behavior coverage.
Old fixtures that used nonzero exit specifically to select ERR should
express that intent with fail; orderly nonzero exits need their own cases.
Preserve the underlying assertions, use fresh IDs for changed cases and
predict additions/retirements from the source and spec.db history.

A pre-change unknown-command refusal for fail does not prove its routing,
non-returning behavior, status preservation or taint integration. Use
focused reversions after recognition exists, including independent source
and SSA input checks and the runtime selection/progression mechanism.
Keep the normal gate and commit hooks; coverage remains paused.

Implementation is authorized on `exit-fail-return-kinds`, followed by
`enforce-function-return-kinds`, `retire-return-ceremony-check` and the
already-settled direct-stage integer/status boundary. No unrelated backlog
or unresolved language-policy change is part of this task.
