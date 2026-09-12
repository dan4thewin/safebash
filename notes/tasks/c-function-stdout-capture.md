---
title: "c: function stdout capture ($(f)) unimplemented — CaptureStart/End TODO"
status: ready
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "c: SsaInst::CaptureStart/CaptureEnd are emitted (redirect fn stdout into a buffer, store into the target), not `/* TODO inst */` — `s=$(f)` and `echo $(f)` capture f's stdout"
  - "janet: `$(f)` on a user function captures the function's stdout (inline/with-out-str), not `(os/spawn [\"f\"])` exec of the name"
  - "un-guard spec/functions.sh 17.3 / 17.4 to include c and janet"
---

# c/janet function stdout capture is unimplemented

Surfaced 2026-08-14 completing `string-func-capture-in-arg-ice` (native lift via
the shared `capture_func_into` / CaptureStart-End machinery). native works in
both the two-step (`s=$(f)`) and inline arg (`echo $(f)`) forms. The oracle
backends do not:

- **c** (`emit_c.rs`): `SsaInst::CaptureStart` (Discriminant 11) and
  `CaptureEnd` (12) are emitted as `/* TODO inst */`. The function body runs
  straight to fd 1 and the capture target stays `""`. So `echo $(greet)` prints
  greet's stdout directly and the substituted value is empty — passing only by
  accident for the bare case (trailing-newline strip) and failing for concat
  (`echo "hello-$(greet)"` → `world\nhello-` instead of `hello-world`).
- **janet** (`emit_janet.rs`): a `$(f)` capture emits `(os/spawn ["f"] …)` —
  it execs the function *name* as an external command instead of inlining the
  body. (Same class as janet:17.1, already failing.)

Guarded `spec/functions.sh` 17.3 / 17.4 to native meanwhile. The native
transform (`capture_func_into`) is backend-shared; only the codegen of
CaptureStart/CaptureEnd is missing in c/janet.


## Rescoped to c-only (DD#104, 2026-09-12)

The janet backend is removed. This task was filed against both
backends; the **c** half is still real and still open — it shows up
as one of the c-mode skips. The janet half is gone, not fixed.
