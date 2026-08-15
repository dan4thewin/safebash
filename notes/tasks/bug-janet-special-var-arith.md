---
title: "bug: janet backend can't use `$$` (pid) as an arithmetic operand — emits unknown symbol `$`"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`echo $(( $$ > 0 ))` compiles + runs on the janet backend (currently a janet compile error)"
  - "parity with native/c, which compute the pid; specs 31.20-24 can widen from `native,c` to all modes"
  - "no janet regression"
---

## Symptom

On the janet backend, `$$` (pid) as an arithmetic operand emits a reference to
a bare janet symbol `$`, which janet cannot resolve:

```
$ sash --janet -c 'echo $(( $$ > 0 ))'
...
(print (string (if (> (_num $) 0) 1 0)))
$ janet …
compile error: unknown symbol $
```

Native and c compute the pid correctly (native fixed via `lower_value_as_int`;
c emits `sash_getpid()`). The spec tests for special vars as arithmetic
operands (arithmetic.sh 31.20–24) are therefore guarded `in_modes native,c`;
once janet computes `$$`/`$?` in arith they can widen to all modes.

## Root cause (to confirm)

emit_janet lowers an arithmetic `DollarVar("$")` to a bare janet symbol instead
of a `(sash-getpid)` call (and `$?` likely to `__last_exit`, which may already
work — verify). The janet arith lowering needs the same computed-special
handling the native/c paths have.

## Scope

Part of the janet backend-parity effort (see janet-join-intrinsic-gaps,
cjanet-function-stdout-capture). Native is the gate; this is a janet-only gap.
