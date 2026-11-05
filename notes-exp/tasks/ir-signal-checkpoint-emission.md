---
title: "Test checkpoint emission against IR signal handling"
status: open
kind: testing
depends: []
gates:
  - "the IR reader and C++ emitter are implemented"
  - "signal-free IR emits no checkpoint calls"
  - "IR with cooperative EXIT signal handling retains the required checkpoints"
  - "all emitted poll paths respect DD128 acquisition/resolution windows"
---

DG requested this future consumer regression test after noticing unnecessary
checkpoints in signal-free hand-written examples. Add it once the transpiler
exists; do not introduce an IR reader or change the current runtime for
this task.

[DD#15](../decisions/DD15-checked-exit-explicit-err.md) adds the runtime's
explicit compiler-owned deferral flag. Native DD128 makes placement inside
protected windows part of this future consumer regression too.

Exercise actual emitted C++ from focused IR fixtures:

- Signal-free IR containing functions, loops, and commands emits no
  checkpoint calls.
- Signal-free IR containing literal text such as `signal` or
  `rt::checkpoint()` still emits none. Detect actual IR operations and
  generated calls, not matches inside strings or comments.
- IR installing cooperative EXIT signal handling retains checkpoints at
  the required execution boundaries. Include registration inside a called
  function so a top-level-only scan cannot pass.
- A pending EXIT signal during acquisition updates signal state immediately
  but cannot dispatch before the legal adjacent attest or its direct
  failed-branch replacement. Include a single-use function return and
  capture, not just two neighboring main statements.
- Reader conditions and completion, including a false-reader edge, retain
  their actual polls without inventing a successful body attest. Cover
  function-return and external-command/pipeline polls as well as explicit
  checkpoint instructions.
- Ordinary failure and explicit zero/nonzero exit inside a protected window
  retain their own routes; a pending signal cannot turn failure into EXIT.
- Signals received during either terminal handler update their information
  without redispatch or re-entry.

Derive the decision from the whole program IR, not sash source text or
an unrelated emitter option. The positive case must reject an emitter
that omits every checkpoint; the negative case must reject an emitter
that inserts checkpoints unconditionally.
Also require the placement gate to reject ignored deferral and omitted
post-reader/function-return polls. Assert execution ordering and completed
values as well as generated calls; counting calls cannot prove protection.

This checks calls added to generated code, not `run`'s fixed internal
lifecycle checks. It belongs to the consumer work on the
[runtime roadmap](cpp-transpiler-runtime-before-ir.md), not the runtime's
completion gate.
