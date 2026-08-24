---
title: "c-backend parity: argv-index read should be is_int-total (emit_c.rs:1244)"
status: ready
priority: P3
kind: parity
depends: []
gates:
  - "emit_c_argv_value reads an int index var directly (no sash_atoi on a nonexistent _ptr/_len) — parity with native index_var_to_int"
  - "no regression"
---

## Context

The native argv/table index-var read is now `is_int`-total
(`codegen/values.rs::index_var_to_int`, journal 173): an int slot is read
directly, a string slot is `atoi`'d. The C backend's `emit_c_argv_value`
(`emit_c.rs:1244`) still emits `sash_atoi({iv}_ptr, {iv}_len)`
unconditionally — it assumes the index var (`__fi`) is a string.

Today `__fi` is always a string var in both backends, so the emitted C is
correct (a legitimate string→int) — this is a **parity gap, not a live bug**.
But if `__fi` ever becomes an int counter (the efficiency change the native fix
now permits), the C output would reference nonexistent `{iv}_ptr`/`{iv}_len` on
an int var → a C compile error.

## Fix

Thread `types` into `emit_c_argv_value` (and the table-index C paths if they
share this) and, when the index var is `VarType::Int`, use the i64 var directly
instead of `sash_atoi(_ptr,_len)` — mirroring `index_var_to_int`.

## Provenance

Found by the atoi audit (journal 173, DG follow-up "see that the only atoi is
within string interpolation"). Native is clean; this is the C twin.
