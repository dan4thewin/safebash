title: DD#56 — implement flag-only traps (trap : SIG)
status: ready
depends:
  - dd56-signum-keyword
severity: feature
gates:
  - tests
  - commit

`trap : SIG` registers a signal handler that sets `sash_signum` to
the signal number. Program resumes. No compiler-inserted checks.

### Runtime

Add `sash_signal_handler` — async-signal-safe, writes signal number
to `sash_signum`. Add `sash_trap_flag(int signum)` that calls
`sigaction(2)` to register `sash_signal_handler` for the given signal.

### Transform

`trap : SIG` → emit `__trap_flag(signum)` SSA instruction.
Partially implemented already as `__trap_signal` with handler=1
(SIG_IGN). Rework to call the new flag handler instead.

### Codegen

`__trap_flag(signum)` → call `sash_trap_flag` runtime function.

### Lowerer

Recognize `trap : SIGNAME` syntax. Map signal names to numbers
(existing table). Coerce `trap '' SIG` (bash ignore) to `trap : SIG`.
