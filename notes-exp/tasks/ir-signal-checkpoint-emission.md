---
title: "Test checkpoint emission against IR signal handling"
status: open
kind: testing
depends: []
gates:
  - "the IR reader and C++ emitter are implemented"
  - "signal-free IR emits no checkpoint calls"
  - "IR with cooperative EXIT signal handling retains the required checkpoints"
---

DG requested this future consumer regression test after noticing unnecessary
checkpoints in signal-free hand-written examples. Add it once the transpiler
exists; do not introduce an IR reader or change the current runtime for
this task.

Exercise actual emitted C++ from focused IR fixtures:

- Signal-free IR containing functions, loops, and commands emits no
  checkpoint calls.
- Signal-free IR containing literal text such as `signal` or
  `rt::checkpoint()` still emits none. Detect actual IR operations and
  generated calls, not matches inside strings or comments.
- IR installing cooperative EXIT signal handling retains checkpoints at
  the required execution boundaries. Include registration inside a called
  function so a top-level-only scan cannot pass.

Derive the decision from the whole program IR, not sash source text or
an unrelated emitter option. The positive case must reject an emitter
that omits every checkpoint; the negative case must reject an emitter
that inserts checkpoints unconditionally.

This checks calls added to generated code, not `run`'s fixed internal
lifecycle checks. It belongs to the consumer work on the
[runtime roadmap](cpp-transpiler-runtime-before-ir.md), not the runtime's
completion gate.
