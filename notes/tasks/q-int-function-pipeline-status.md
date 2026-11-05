---
title: "Question: integer function results used as pipeline process statuses"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG settles whether an int-valued function may implicitly supply a pipeline stage's process status, or the call must be rejected or explicitly adapted"
  - "the chosen contract is stated consistently in DD58/DD55, without weakening DD74's rejection of int functions as predicates"
  - "focused native/C specs distinguish return 3 from return 256 and preserve ordinary integer captures"
---

## Finding and scope

Batch-531 B79; B75 is its duplicate. The reference did not fix it and it
blocks no port work. DG's backlog rule therefore leaves behavior unchanged
at closeout (journal 570). This is the remaining stage-result question, not
another unported hunk or the already-fixed B7/B15 predicate/call checks.

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

## Decision needed

DD58 distinguishes integer values from status answers; DD74 rejects an
int function used as a predicate. DD55 also gives each pipeline stage its
own process status and checks the aggregate. The implicit conversion of an
integer function's answer at that boundary needs an explicit policy.

Rejecting this stage call preserves the value/status distinction but needs
a wrapper for functions used for their stdout. Retaining the conversion
preserves current programs but makes integer values such as 256 look like
success; that narrowing must be intentional and documented. An explicit
adaptation/discard form would be a further language choice, not an incidental
backend fix. No choice is made by this filing.

Evidence: `files/port-closeout-probes.tar.gz`, cases `b79-stage-256`,
`b79-stage-3` and `b77-int-call-control`, with complete mode xtraces.
