# 200 — purge `trap` from native (DG); file exec-support

DG's disposition of the two "accepted-in-source bash forms" (journal 199):
- **`trap`** — purge from native. Done here.
- **`exec`** — NOT dead; it serves two real use cases (fd manipulation of the
  running program; program replacement) with no native replacement. We must
  **support** it. Filed `support-exec-builtin` (P1); the existing exec codegen
  is unblocked-and-finished there, not removed.

## trap purge

`trap` was quietly accepted in sash source and transformed to the native signal
primitives inside the compiler (`handle_trap` → `TrapSignal`/`SignalFlag` /
`exit_trap`). DD#56 makes `signal N {IGN,DFL,FLAG,EXIT}` + `END { }` the native
model and `trap` a bash form the **converter** rewrites — so the in-compiler
`trap` alias duplicated the converter's job and let a non-native spelling into
source. Removed, mirroring the `command` purge (DD#23, 67affbc).

- `validate.rs`: new `check_trap` → **SB4043** ("`trap` is not part of sash —
  use `signal`/`END`; the converter rewrites bash `trap`"); dropped `"trap"`
  from the `BUILTINS` list.
- `ssa.rs`: removed `BuiltinCmd::Trap` + its `from_name` arm.
- `transform/stmt_handlers.rs`: removed the `BuiltinCmd::Trap` dispatch.
- `transform/cmd_handlers.rs`: deleted `handle_trap`, `handle_trap_exit`,
  `handle_trap_ignore_reset`, `is_ignore_reset_body`, `signal_name_to_num`,
  `strip_outer_quotes` (~90 lines, all trap-specific).

**Kept (shared native-signal machinery):** the `TrapSignal`/`SignalFlag` SSA
sigils + their codegen (used by the `signal` builtin), and `exit_trap` + its
entry.rs reader (used by `END { }` via statements.rs — NOT trap-specific).

## Verification

`trap … EXIT` / `trap "" SIG` / `trap - SIG` → SB4043 (all backends — it's a
validation reject). `signal N …` + `END { }` unchanged; converter trap→signal
(signals.sh 57.14) unchanged. specs signals.sh 56.14/56.15. DD#56 amended.
0 native/c regressions.
