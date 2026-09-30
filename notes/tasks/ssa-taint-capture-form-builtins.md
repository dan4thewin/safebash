---
title: "SSA taint backstop: capture-form builtins decide taint before taint analysis runs"
status: open
priority: P2
kind: hardening
depends: []
gates:
  - "under `--no-taint-cst` a builtin capture whose operands are tainted (`x=$(join @a)` for a tainted `a`) is rejected by the SSA backstop"
  - "0 spec regressions with the CST pass ON — a clean builtin capture must not false-positive"
  - "spec `taint_switch.sh` gains sw.* cases for the capture form"
---

Split out of `ssa-taint-full-backstop.md` on 2026-10-01, when that task's other
two gaps closed. This is the last one.

`$(join @a)` / `$(echo …)` lower via `CaptureStart` / `CaptureEnd { target,
tainted }`, not `SsaValue::CmdSubst`, so `cmd_subst_taint` never sees them.
`CaptureEnd.tainted` is a **boolean fixed at transform time** — external (exec)
→ true, builtin → false — which is before taint analysis has run. A builtin
capture is therefore Clean no matter what it reads.

Fixing it means either taint-checking the captured command's operands, or
deriving `CaptureEnd`'s taint after analysis instead of during lowering.

Not an escape: the CST pass rejects every one of these SB5020 in a normal
compile, which is the condition DG's 2026-09-12 deferral rests on. Re-verify
that condition before picking this up, and treat any CST hole as reopening it
urgently — nothing else catches this shape.

Worth knowing before starting: the fix for the bulk fills (the sibling task)
worked by giving each fill a real SSA version *after* the autodeclare, so the
existing max-version rule in `taint_of_name` produced the right answer with no
special-casing. If `CaptureEnd` can be made to derive its taint the same way —
from the values it reads, at analysis time — the same leverage should apply.

If you do change how a value reaches codegen, check `map_inst_versioned`
(`codegen/setup.rs`): an SSA version it does not map makes `load_var_or_empty`
return a **null pointer silently**, which segfaults rather than ICEs. That cost
53 spec failures when the bulk fills landed.
