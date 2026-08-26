# 186 — taint bypass switches (DD#82): --no-taint / --no-taint-cst

DG's concern: taint quarantine is the headline safety property and the most
likely thing a public fork rips out — a "taint-free" fork could overshadow the
original. A *supported* bypass removes the incentive to fork. And the two-layer
split doubles as the way to **test the SSA taint backstop in isolation**: with
the CST front-end pass off, the SSA pass becomes the active gate.

## Switches (DD#82)

- `--no-taint-cst` — disable the CST front-end taint pass (`check_taint`); the
  SSA backstop still runs (native only).
- `--no-taint` — disable ALL taint checking (CST + SSA). Full escape hatch.

`--no-taint` subsumes `--no-taint-cst`. The switches disable *taint only* —
every other language rule still applies (`--no-taint` + `$@` is still SB5045).

## Implementation

`lib.rs`: `cst_taint_disabled(flags)` (`--no-taint` ∨ `--no-taint-cst`) gates
`check_taint` in `run_semantic_checks`; `ssa_taint_disabled(flags)`
(`--no-taint`) gates `verify_ssa_taint` in `compile_native`. `parse_cli_args`
already collects any `--`flag, so no parser change was needed.

Spec harness: `SASH_CFLAGS` (common.sh) injects compile flags into all three
mode runners; the native/c/janet err capture now also surfaces the SSA taint
rejection line.

## What the SSA backstop catches TODAY (before the DD#82 hardening)

Verified under `--no-taint-cst` (native):
- assigned scalar source: `x=$(getenv HOME); echo $x` → rejected ✓
- value propagated through echo: `x=$(echo $y)` (y tainted) → rejected ✓
- but NOT yet: `x=$(join @a)` (cmd_subst_taint hardcodes join=clean), and NOT
  argv/mapfile array taint (SSA defers arrays to CST). Those are the next
  step (journal 187 / task `ssa-taint-cmdsub-operand-propagation`).

## Spec

`spec/taint_switch.sh` sw.1–sw.7: default rejects (control); `--no-taint`
compiles+runs a tainted scalar (sw.2) and join (sw.3, native+c), leaves `$@`
rejected (sw.6) and clean programs unaffected (sw.7); `--no-taint-cst` (native)
SSA still rejects an assigned source (sw.4) and an echo-propagated value (sw.5).

Full `SPEC_MODES="native c janet"` sweep: 0 regressions, native gate 0, 15 new
passing (sw.* across modes).
