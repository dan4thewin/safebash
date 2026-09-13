---
status: done
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

## Fixed 2026-09-13 (commit follows journal 287)

Re-measured: confirmed exactly as filed. aarch64 `_start` had
`bl sash_init_heap` and no `bl sash_trace_init`.

Fixed by taking the third gate seriously rather than adding the missing
line. The startup sequence now lives in **one** place — a plain Rust
`sash_startup()` that calls `sash_init_heap` then `sash_trace_init` — and
each `_start` does a single `call`/`bl sash_startup`. The two assembly
bodies no longer enumerate init steps, so they cannot disagree about them
again. Adding an init step is now a one-line edit in Rust.

Both symbols turned out to be referenced **only** from the two `_start`
blocks (nothing in `sash/`, `emit_c.rs`, or the spec suite), so the C
backend needed no change: it links `libsash_rt.a` and uses the same
`_start`.

### Verified

- x86_64 `--trace` end to end: `SASH_TRACE=1` on a `--trace` binary emits
  `+ branch → if.then`; unset emits nothing. `tr.1`–`tr.3` pass.
- Full sweep 4330 passing, 9 skipped, 0 failing, 0 flips. This is the
  broad gate that matters here: *every* compiled binary now goes through
  `sash_startup`, so a mistake in it would have been a mass failure.
- `sash-rt` warning count unchanged (10 before, 10 after).

### NOT verified, and cannot be from this host

The gate "`tr.1`–`tr.3` pass on aarch64" is **unmet**. This machine is
x86_64 with no aarch64 hardware, no `qemu-user`, and no
`aarch64-linux-gnu-gcc`; `sash-rt` vendors C sources (Plan 9 regex,
heap4, `setjmp.S`) through `cc-rs`, so even a compile-only cross build
needs that toolchain, and installing it needs root, which this session
does not have. Adding the rustup target alone is not sufficient (it was
added, tried, and removed again).

So the aarch64 change — `bl sash_init_heap` → `bl sash_startup` — is
reviewed but not executed. The risk is low (both are `#[no_mangle]
extern "C"` symbols and `bl` to either is the same instruction form, so
there is no new assembly shape), but it is not zero, and it should be
run on real aarch64 hardware before that gate is called met.

No spec was added. The suite runs on x86_64 only, so a spec for this bug
could not fail on the architecture that runs it — and the drift is now
prevented structurally rather than detected. No existing spec reads repo
source, so a source-invariant test would also be without precedent here.
