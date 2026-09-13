---
status: ready
priority: P3
kind: bug
depends: []
---

# aarch64 `_start` never calls `sash_trace_init`, so `--trace` is dead there

`sash-rt/src/lib.rs`, the two `_start` implementations:

- **x86_64** calls `sash_init_heap` **and** `sash_trace_init`.
- **aarch64** calls `sash_init_heap` only.

`sash_trace_init` is what reads `SASH_TRACE` from the environment and sets
the global `sash_trace_flag`. Without it the flag stays 0, so every
trace-write site compiled in by `--trace` (DD#32) tests false and emits
nothing. On aarch64, `sash --trace` therefore produces a binary that is
larger, slower, and silent — a feature that reports success and does
nothing, which is the failure mode this project treats as worst.

Not caught because the spec suite runs on x86_64, and `spec/trace.sh`
(`tr.1`-`tr.3`) is the only coverage.

## Fix

Add the `sash_trace_init` call to the aarch64 `_start`, alongside the
existing `sash_init_heap` call. The ordering constraint is the same one
x86_64 already satisfies: `sash_envp` must be stored first, which it is.

## Gates

- aarch64 `_start` calls `sash_trace_init`.
- `tr.1`-`tr.3` pass on aarch64.
- Ideally the two `_start` bodies stop being able to drift: the
  divergence is only possible because the startup sequence is written
  twice, once per architecture.

## Found

2026-09-13, reading `_start` while implementing DD#107's heap
reservation — DG asked whether `SASH_TRACE` was implemented at all.
