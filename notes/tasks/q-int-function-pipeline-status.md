title: "Reject int-function calls as status-producing pipeline stages"
status: complete
priority: P2
kind: bug
depends: []
gates:
  - "direct int-function stages are SB4030 in every otherwise legal stage position, marking and status-handling context"
  - "ordinary integer captures, int-to-int return forwarding, in-process value discard and explicit numeric/predicate status construction remain valid"
  - "focused native/C specs reject both return 3 and return 256 as direct stage results, with before-controls and the ordinary unit/spec gate"

## Settled policy, implemented

Implemented and verified in journal 577. The existing definition-in-force
classifier supplies the direct-stage SB4030 guard. Six focused checks per
backend turn red when that guard alone is reverted; integer captures,
in-process discard and explicit-status wrappers remain valid. The final
ordinary gate passes 11345 checks with no failures.

DG ruled on 2026-11-05:

> in my mental model, a status and int result are essentially different types;
> treating an int function's returned value as a status should be the same
> family of error as illegal mixing of string and int. thus, suppression is
> not appropriate

DD58/DD55/DD74 now state the distinction. A direct int-function stage is
SB4030, even with `-`; suppression cannot repair a type mismatch. The same
principle rules out DD55's former `return int_fn` coercion in a non-int
function. A wrapper must discard the integer as a value or deliberately
produce a status, not merely forward the value into a status position.

The separate in-process `- int_fn` value discard remains valid: no status
conversion is involved. Integer captures and compatible integer return
forwarding are unchanged. Explicit numeric status construction and status
tests keep their existing contracts.

This file keeps its old slug for incoming links, but is now an
implementation bug rather than an unresolved question. As of 2026-11-06, the return-producer
consequence is owned by `enforce-function-return-kinds`; this task owns
direct pipeline-stage use. Neither task's completion implies the other is
done. Both are now implemented and execution-probed in journal 577.

## Finding and scope

Batch-531 B79; B75 is its duplicate. The reference did not fix it and it
blocks no port work. DG's backlog rule therefore left behavior unchanged
at closeout (journal 570). This is an independent typing repair, not another
unported hunk or the already-fixed B7/B15 predicate/call checks.

Current-tree observations on `09a5bd49`, identical in native and C:

```sh
fn f -> int { echo hi; return 256 }
f | readline s
echo "[${s:!}]"
```

This prints `[hi]`, exit 0. Changing only `return 256` to `return 3`
aborts with `f: command failed (exit 3)`, exit 3, before the final echo.
The function's integer answer has become the child's process status and
undergone the process boundary's 8-bit narrowing. `:!` merely isolates this
question from the reader's independent taint obligation; the original
integer-return spelling remains valid.

The normal value-call control, `fn f -> int { return 7 };
int n=$(( f )); echo "$n"`, still prints 7.

## Implementation scope

Enforce the function-result type before lowering it into a process status,
not by special-casing nonzero values or changing the eight-bit exit mapping.
Cover bare, suppressed and negated stages, handled pipelines, and wrapper
forms without weakening DD74's predicate rejection. Use the existing
definition-in-force classification; do not duplicate return-kind inference.
A process having its own status does not make an int function's answer a
status. Return-producer compatibility is tracked separately in
`enforce-function-return-kinds`, not deferred from that task back into this one.

Do not reject an ordinary value use merely because its enclosing body runs
in a child. Pin a void wrapper's value discard and a status wrapper's
explicit predicate alongside the integer-capture preservation controls.

Evidence: `files/port-closeout-probes.tar.gz`, cases `b79-stage-256`,
`b79-stage-3` and `b77-int-call-control`, with complete mode xtraces.
