title: DD#82 — Taint bypass switches (--no-taint, --no-taint-cst)
status: implemented

1. `--no-taint` disables both frontend and SSA taint checking as an explicit
   trust bypass, not validation of external data.
2. `--no-taint-cst` disables only frontend/AST taint checking, retaining
   the independent SSA backstop for native and C emission.
3. Both switches leave parsing, structural validation, types, arity,
   naming, collection non-aliasing and other non-taint rules in force.

---

## Problem

Taint quarantine (DD#24/#61) is the project's headline safety property, but
it is also the most likely thing a public fork would rip out — and a
"taint-free" fork could overshadow the original. A *supported* bypass removes
that incentive: someone who doesn't want the checks flips a switch instead of
forking. The same layered switch doubles as the mechanism to exercise the SSA
taint backstop in isolation (otherwise the CST pass rejects everything first,
so the SSA layer is never reached in a test).

## Decision (DG 2026-08-26)

Two compile-time switches, matching the two taint layers:

- **`--no-taint-cst`** — disable the ~~**CST**~~ (front-end) taint pass
  (`validate::check_taint`). The **SSA** taint backstop still runs ~~(native
  only).~~ Purpose: (a) a lighter bypass, and (b) the way to test the SSA layer
  — with the front-end pass off, the SSA pass becomes the active taint gate.
- **`--no-taint`** — disable **all** taint checking (both the ~~CST~~ AST pass and the
  SSA backstop). The full escape hatch.

`--no-taint` subsumes `--no-taint-cst` (both cause the ~~CST~~ AST pass to be skipped;
`--no-taint` additionally skips the SSA pass).

### Scope — taint only

The switches disable *taint* checking, nothing else. Structural validation,
`$@`/`$*` rejection (SB5045), positionals, arity, type checks, and every other
language rule still apply. `--no-taint` compiles a program with tainted data
flowing freely; it does not turn sash into bash.

### Backend note

~~The SSA taint pass runs in the native backend only (`compile_native`). So
under `--no-taint-cst`, the c/janet backends have no taint layer left and
behave as if fully bypassed; the SSA backstop is a native-mode guarantee.~~
DD#127/DD#128's independent SSA check covers native and C compilation;
DD#104 replaces the in-tree Janet backend with the external IR seam.

## Implementation

- `lib.rs`: `cst_taint_disabled(flags)` (`--no-taint` ∨ `--no-taint-cst`) gates
  `check_taint` inside `run_semantic_checks`; `ssa_taint_disabled(flags)`
  (`--no-taint`) gates `verify_ssa_taint` ~~inside `compile_native`~~. The arg
  parser already collects any `--`flag, so no parser change was needed.
- Spec harness: `SASH_CFLAGS` (common.sh) injects compile flags into all three
  mode runners so tests can exercise the switches.

Spec: `spec/taint_switch.sh` (`sw.*`).
