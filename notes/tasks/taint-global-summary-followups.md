---
title: "Global-summary precision follow-ups"
status: open
priority: P2
kind: question
depends: []
gates:
  - "gflow.20-.22 retain their located SB5020 diagnostics, including under each merge reversion control"
  - "Any ordered transfer checks each read before a later legal resolution and does not apply skipped-call effects"
  - "Ordinary clean overwrites remain unresolved as DD128 requires"
---

Recorded with W1b-1, 2026-11-02 (journal 557). The unit uses a conservative
union for captures and arithmetic/value calls. This prevents laundering by
a skipped call or by a call in a later operand, but also preserves incoming
taint after a guaranteed eager clean overwrite.

DD128 supersedes the requested clean-overwrite repair (journal 568).
Both programs below are intentional refusals now, even when the overwrite
definitely executes. This file remains open only for the separately listed
precision questions; it does not authorize ordinary-write declassification.

## Former false positives, now intentional

Both programs get SB5020 on the final read of `x`, on the pre-W1b-1 compiler
and the globals-summary compiler. With `--no-taint`, the new native compiler
produces `safe 7` and `safe ok`, respectively.

```
str x=clean
fn clean -> int { x=safe; return 7 }
x=$(/bin/echo dirty)
int n=$(( clean ))
echo "$x $n"
```

```
str x=clean
x=$(/bin/echo dirty)
str q=$(x=safe; echo ok)
echo "$x $q"
```

The errors are at 5:6 and 4:6. Evidence:
`w1b1-eager-value.*` and `w1b1-eager-capture.*` in the session's
`files/port-w1b1-probes.tar.gz`.

Do not replace the union with an unconditional assignment. That was tried
as an independent reversion: the capture change loses `gflow.20`, and the
arithmetic change loses `.21`/`.22`. A future ordered-expression transfer
must preserve those refusals and DD128's strict resolution. Fresh acquisition
and legal resolution, not these ordinary overwrites, would be the relevant
precision case.

## Follow-up probes, not established bugs

The implementation has other conservative seams worth checking before
extending it. These are questions recorded during W1b-1, not claims that a
valid source program has demonstrated another failure:

- Duplicate-definition precision is no longer a supported-language question:
  DG rejects all repeated function names (DD86, journal 582).
  `reject-duplicate-function-definitions` owns enforcement before the
  name-keyed census/cache; do not specialize summaries to support replacement.
  Until that guard lands, the existing union remains an implementation fact.
- Return-producer traversal and a capture's final-state union may include
  states that do not actually fall through. Keep explicit returns separate
  when testing this; `gflow.13` already pins the named-function exit union.
- Illegal inter-function recursion can encounter partial summaries while
  the graph is being built. A probe must remain compile-only and retain the
  real recursion refusal; a zero-progress legal self-tail loop never
  terminates at runtime.
- W1b-2 implements independent slot transfers (journal 559); `len A` still
  accepts received taint. Slot/slot and slot/global aliasing are settled by
  DD100 and enforced by SB5099 (journal 558). The mapper omits a passed
  global's unused identity flow so it cannot mask cleanup through the slot.
  Existing integer self-tail collection refusals and the separately filed
  missing-slot sexpr arity bug are unchanged; the latter was rechecked on
  both compilers, including inside a function body.

Ordinary loop exits are implemented in W3 (journal 562); W4 fixed the
stage-write SSA false positive (B18), and handler entry remains W5. A redirect on the final
`while` was also checked:
it is rejected by SB4021 before taint analysis, so that spelling is not a
missing parent-reader case. The separate, intra-process SSA array-timing
fault exposed by the retired `sflow.5` is fixed in journal 568:
`sflow.19` now requires acceptance. Unlowered redirect operands and the
completed-parent view are not covered by that timing repair.
