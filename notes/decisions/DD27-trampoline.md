title: DD#27 — Trampoline Function Model
status: implemented

Functions are blocks within the single Cranelift `_start` function, not
separate Cranelift functions. Call = branch, return = branch back.
Positional params saved/restored around calls. Enables inlining without
ABI overhead.
