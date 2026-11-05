---
title: "DD#5 - Planned END-only explicit failure handling"
status: superseded
---

Superseded by [DD#15](DD15-checked-exit-explicit-err.md) after DG directed
adoption of implemented native DD128. END is removed; checked EXIT and
explicitly handled ERR replace it. The remainder is the historical
decision and its original prototype milestone, not the current contract.

DG expects two changes to END:

- No source function definitions within END.
- Every fallible operation in END must be explicitly caught or suppressed.
  END supplies neither automatic abort nor automatic suppression.

DG clarified that nothing outside END changes. This is the target contract
for the forthcoming C++ lifecycle slice, not a claim that the native
compiler or this runtime already implements it. Current
[native DD#56](../../notes/decisions/DD56-signal-trap.md) still describes
automatic diagnose-and-continue cleanup.

## Scope and consequences

The explicit-handling requirement is END-only. Ordinary function bodies,
commands, captures, arguments, return/result handling, and all other
non-END behavior keep their existing rules. Calling an outside function
from END does not rewrite its body or give it inherited unchecked state.
Functions can still be called from END; their source definitions belong
outside it.

DD#55's no-reach rule is unchanged: `- f` addresses only f's returned
int/status, not its body or argument failures. The new END requirement
does not silently broaden that discard.

The AI's proposed automatic per-step cleanup guards are withdrawn. The
runtime or emitter must not insert an implicit catch-and-continue merely
because an operation appears in END. Explicit source handling may use
ordinary C++ result handling or explicit exception boundaries.

An unhandled fallible END operation is invalid under the planned contract;
it is not automatically turned into an abort or a discarded failure. The
future front end owns enforcing that source requirement. The hand-written
C++ prototype must demonstrate the explicit choices without a temporary
parser or validator.

The other lifecycle rules remain: END belongs to the main process, runs
at most once, and preserves the triggering status unless explicitly
overridden. This decision changes neither signal policy nor non-END
failure policy.

## Prototype milestone

The hand-written C++ lifecycle slice now demonstrates this contract:
the driver handles typed exits but does not automatically catch END
operation failures. Its sixth pair explicitly catches or discards each
fallible END operation and keeps source definitions outside the block.
Invalid unhandled C++ END exceptions escape to the caller.
The source-enforcement requirement remains future front-end work; no
native behavior changed. See [journal 011](../journal/011-lifecycle-runtime.md).
