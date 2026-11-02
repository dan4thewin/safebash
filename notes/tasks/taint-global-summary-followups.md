---
title: "Global-summary precision follow-ups"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "Guaranteed eager overwrites clear inherited global taint; native and C outputs remain safe 7 and safe ok"
  - "gflow.20-.22 retain their located SB5020 diagnostics, including under each merge reversion control"
  - "An ordered transfer checks each read before a later call can clear its taint, and does not apply skipped-call effects"
---

Recorded with W1b-1, 2026-11-02 (journal 557). The unit uses a conservative
union for captures and arithmetic/value calls. This prevents laundering by
a skipped call or by a call in a later operand, but also preserves incoming
taint after a guaranteed eager clean overwrite.

## Measured false positives

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
arithmetic change loses `.21`/`.22`. The needed precision is an ordered
expression transfer, with lazy branches joined, not a different adjacency
rule or a weaker diagnostic.

## Follow-up probes, not established bugs

The implementation has other conservative seams worth checking before
extending it. These are questions recorded during W1b-1, not claims that a
valid source program has demonstrated another failure:

- The census and summary cache are keyed by name. Multiple definitions are
  treated as shared and their read/flow summaries unioned, rather than
  selected by the definition in force at each call.
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

Ordinary loop exits are implemented in W3 (journal 562); the stage-write SSA
false positive is B18/W4, and END entry remains W5. A redirect on the final
`while` was also checked:
it is rejected by SB4021 before taint analysis, so that spelling is not a
missing parent-reader case. The separate, intra-process SSA array-timing
fault exposed by `sflow.5` is
`bug-ssa-taint-array-read-uses-future-version`; W4's process partition alone
will not fix it.
