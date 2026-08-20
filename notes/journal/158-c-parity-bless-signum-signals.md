# 158 — c parity (batch 1): bless-fail abort, signum, DD#56 signals

DG: "tackle all the c parity work, and wait on janet." Inventory: 13 failing
+ 63 native-guarded c tests across ~13 areas. This batch closes three:
bless-failure abort, the `signum` intrinsic, and the DD#56 signal/END group.
0 c regressions, +9 passing (`c:24.12`, `c:sx.7`, `c:56.4-56.11`).

The signal + regex runtime primitives already live in `sash-rt` (shared via
`libsash_rt.a`), so this was emit_c porting, not new runtime.

## bless-failure abort (24.12)

emit_c had **no `Bless` arm** — `bless x [[ … ]]` fell through to the
`/* TODO inst */` no-op. Taint is compile-time, so most bless tests pass
regardless (the var still prints); only 24.12 (an unconditional bless whose
gate can never match must abort) failed.

Added the `SsaInst::Bless` arm mirroring native `emit_bless`: evaluate the
gate via the existing `lower_test_expr`; on failure call the (noreturn)
`sash_bless_fail(name, val)` runtime diagnostic, which exits 1. Key
simplification: `c_var_name` collapses SSA versions to one c slot, so the
native source→target clean-version copy is a no-op in c. (The catchable
`if bless` form lowers to Test + BlessRetag, never a `Bless` inst, so this
arm is always the hard-abort form.)

## signum (sx.7)

`try_native_intrinsic_c` emitted a TODO for `signum`. Added a `signum` arm
that reads the `extern i64 sash_signum;` global through `sash_itoa`. Both
`$(signum)` (cmdsub) and `$(( (signum) ))` (arith sexpr) route through the
same intrinsic dispatch, so one arm covers both. Widened sx.7 `native` →
`native,c` (janet still stubbed, deferred).

## DD#56 signals + END (56.4-56.11)

Four sub-pieces:

1. **signal builtin.** `signal N IGN/DFL/EXIT` → `__trap_signal [sig,
   handler]`; `signal N FLAG` → `__signal_flag [sig]`. Both were
   `/* TODO command */`. Added `BuiltinCmd::TrapSignal`/`SignalFlag` arms
   calling `sash_trap(sig, handler)` / `sash_signal_flag(sig)` (externs
   declared), mirroring native emit_trap_signal / emit_signal_flag. → 56.5,
   56.6, 56.7, 56.10, 56.11.

2. **END preserves the triggering status (56.4).** c's `__exit` used
   `__exit_status`, but the END body's `echo cleanup` reset it to 0, so
   `END { echo }; /bin/false` exited 0 instead of 1. Mirrored native's
   `saved_end_status_var`: declare `__end_status`, capture it on entry to
   the `__end__` block (before the body runs), and make the final `__exit`
   use `__end_status` when an END block exists. `exit N` keeps `__end_status`
   in sync so 56.3 (`exit 42` in END overrides) still wins.

3. **signal EXIT check-point (56.8, 56.9).** After every external command,
   if the program arms a `signal N EXIT` trap (`ssa.exiting_signals`
   non-empty) and a trap'd signal has fired (`sash_signum != 0`), jump to
   the END block with status `128+signum`. Gated so a FLAG-only program does
   NOT early-exit (56.10 stays alive), and skipped once we're at/after the
   END block (a check inside END would re-jump forever) via a new
   `Emit.cur_block` tracker. Mirrors native `emit_signal_exit_check`.

signals.sh is now 17/17 in c.

## Files

- `sash/src/emit_c.rs` — Bless arm; signum intrinsic; TrapSignal/SignalFlag
  arms + `sash_trap`/`sash_signal_flag`/`sash_signum`/`sash_bless_fail`
  externs; `__end_status` (declare + END-entry capture + exit sync +
  final-exit use); `emit_signal_exit_check` + `Emit.{has_exiting_signals,
  cur_block}`.
- `spec/arithmetic.sh` — sx.7 `native` → `native,c`.

## Still failing in c (next batches)

- `55.7/55.8/55.9` — external `$(...)` capture is unimplemented in c
  (`/* TODO cmdsub exec (Phase 4) */`); needs fork/exec/pipe capture + the
  DD#55 checked abort. A status-returning capture helper is the plan.
- `38.5/55.19` — `$!` / background exec: `&` lowers to `__subshell`
  (SubshellStart) + `__store_bg_pid`, both TODO in c (background/subshell
  exec unimplemented). Grouped with subshell work.

## Skipped (native-guarded) c areas still open

bless-all + mapfile (60.x, ad.x), compound pipeline stages (14/15/32.x),
fn stdout capture (17.x, mcs.x), descriptive abort messages (55.20-30),
fn-int-discard (e.b70.fndisc), local int collection (ivc.8), control-flow
gaps, and assorted misc (bng, 63.x, gs.6, ud.16, 24.20, 25.8, 57.46).
