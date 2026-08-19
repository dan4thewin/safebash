---
title: "multi-statement / pipeline command substitution — implemented"
date: 2026-08-19
---

# Multi-statement / pipeline `$(…)` implemented (+ a taint hole closed)

DG (decision, then challenge): implement multi-statement/pipeline `$(…)` — run
the body, capture stdout — and don't defer it just because it's "big" (a fresh
session wouldn't reduce mistake risk; the loaded context is the asset). Done.

`$(x=1; echo $x)` and `$(a | b)` used to silently emit the literal string
`$(...)` at runtime. Now they capture correctly.

## Mechanism (no new codegen)

- `handle_command_substitution` returns a new transform-intermediate
  `SsaValue::CmdSubstBody(body)` when the body isn't a single command.
- `lift_int_func_calls` (the existing hoisting pass, run at every arg +
  assignment-RHS site) grows a `CmdSubstBody` arm → `capture_body_into` → a
  temp `Var` — exactly the `IntFuncCall`/`$(f)` lift pattern. Value-position
  coverage for free (value codegen has no `cl_blocks`; the statement-level
  `CaptureStart/End` inst does).
- `capture_body_into` mirrors `capture_func_into`: `CaptureStart` → lower body
  → `CaptureEnd{target}`. Reuses the existing capture codegen — **no new
  codegen**. Subshell isolation: save/restore every var the body assigns
  (`collect_assigned_vars`), so `$(x=1; …)` doesn't leak `x` to the parent.

## ⚠ The taint hole (why this needed care, not speed)

`cmdsub_is_taint_source` / `cmdsub_is_scalar_taint_source` inspected only
`stmts.first()`. A pipeline/multi-stmt body (external output) was therefore
treated **clean**, and `CaptureEnd` optimistically marks Clean — so a naive
capture would let `$(external | cat)` flow as clean, **bypassing `bless`** (a
validator bypass, the #1 audit concern). Fixed taint-**first**:

1. Both taint-source scans now recurse over **every** statement / pipeline
   stage (`child_bodies()`), so any `/abs` / `getenv`/`argv`/`arg` anywhere in
   the body taints the capture. (Resolution runs before taint-check, so
   externals are `/abs` by then.)
2. `CaptureEnd` gained a `tainted` flag; `capture_body_into` sets it to
   **exactly** `cmdsub_is_taint_source(body)` — matching the CST decision, so
   the SSA-taint backstop neither ICEs (clean body wrongly tainted) nor holes
   (external body wrongly clean).
3. This also closed a **pre-existing** hole: `$(- cmd)` — the `-` suppression
   wrapper hid the external from the first-stmt-only scan, so `x=$(- external)`
   captured external output as clean. Now tainted (spec 55.9 updated to bless;
   commands.sh 14.3e, taint.sh mcs.*).

## Backends

Native: full (functional + isolation + taint). **c/janet**: capture-into-var
is a pre-existing gap — even the existing `$(f)` capture leaks in c (the
`CaptureStart/End` capture pipe) and the Janet cmdsub is partial — so the new
capture tests are `native`-only, matching commands.sh 14.2c. The taint rejects
are compile-time (all modes).

## Verification

spec commands.sh 14.3a-e, taint.sh mcs.1-4, errors.sh 55.9 (hole closure).
3-mode sweep: **0 regressions, +17 passing**. fmt/clippy clean;
`collect_assigned_into` cog3/cc7 (flattened from cog16 to clear the gate).

## Deferred (follow-ups)

- c/janet capture parity (the `CaptureStart/End` capture gap affects `$(f)`
  too) — the c cmdsub Phase-4 TODO + janet cmdsub. Not new to this change.
- Non-variable side effects in a bare multi-statement body (e.g. `$(cd /x;
  ls)`) still run inline — only variable assignments are isolated. Rare;
  pipelines (the common case) fully isolate via their stage forks.
