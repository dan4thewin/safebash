---
title: Status
modified: 2026-05-14
---

## Current branch: split-converter

## Spec status: 282 passing / 3 failing (spec.db)

### Session 2026-05-14 — completed

**Cleanup: Remove SsaInst::Export dead code**
- Removed `SsaInst::Export` variant from SSA, codegen, taint, transform
- Removed `sash_setenv` from runtime (sash-rt)
- Removed `export_basic` and `export_visible_to_child` tests (dead code)
- Validator still rejects `export` (SB5001) — that's the gate

**UX: Convert codegen panics to diagnostics**
- Wrapped `compile_ssa` in `catch_unwind` — ICE panics now produce
  clean error messages instead of Rust stacktraces
- 36 ICE panics remain as safety nets (legitimate internal errors if
  earlier passes have bugs), but users see clean messages

**Feature: DD#23 --manifest (dependency manifest)**
- `sash --manifest script.sh` lists all external command dependencies
- Resolves bare command names via PATH (same as compilation)
- Outputs sorted absolute paths, one per line

**Feature: DD#23 --sysroot (cross-compilation path resolution)**
- `sash --sysroot=/mnt/target script.sh` resolves commands against
  target filesystem instead of build host
- Each PATH component checked with sysroot prepended
- Affects validator, transform, and --manifest

**Feature: DD#23 squashfs sysroot**
- `sash --sysroot=rootfs.squashfs script.sh` resolves commands against
  a squashfs image without mounting it
- Uses `unsquashfs -l` to list files, caches result
- Combined with dm-verity: guarantees compiled binary's dependencies
  match the verified filesystem

**Docs: DD#57 two-tool model**
- Updated README.md for sash + sash-convert split
- Updated GUIDE.md with Two-Tool Model section and repo structure
- Updated SUBSET.md coercions section

**Stale task cleanup**
- Marked done: dd32-runtime-trace, dd57-strip-bash, regex-counted-repetition
- Marked done: compound-redirects (specs already passing)
- Unblocked: dd57-docs (blocker completed)

### Test suite (no regressions)
- e2e: 124 / 0
- oracle: 59 / 0
- taint: 42 / 0
- codegen: 221 pass / 43 fail (pre-existing pipeline/cmdsub)
- spec: 282 pass / 3 fail (DD#56 signal tests)

### Remaining tasks

**Ready:**
- brush-rerun — Brush suite re-run and triage
- fuzz-validator — Fuzz target for validator
- improve-agent-prompt — Improve autonomous agent prompt
- dd23-sysroot-squashfs — done (just completed)

**Deferred:**
- audit-run3 — Adversarial audit run-3 (schedule after significant new code)
