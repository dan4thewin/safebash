title: DD#56 — implement signal SIG FLAG
status: ready
depends: [dd56-signum-keyword]
severity: feature
gates: [56.7, 56.10, 56.11]
---

`signal SIG FLAG` registers a handler that sets `sash_signum`.
Program resumes. No compiler-inserted checks. Zero overhead.

### Runtime
sash_signal_handler (async-signal-safe) writes signal number to
SASH_SIGNUM atomic. `sash_signal_flag(signum)` calls sigaction(2).

### Codegen
`signal SIG FLAG` → call `sash_signal_flag` runtime function.
