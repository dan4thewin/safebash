# 165 — c parity (batch 9): background exec / `$!` / wait

`&` background exec was unimplemented in c: `__subshell` (SubshellStart),
`__store_bg_pid`, and `wait` were all `/* TODO command */`. The SSA block
structure was already right (SubshellStart's fork result drives the Branch:
child block runs the bg body + exits, parent block continues) — only the
command emission was missing.

## Implementation (emit_builtin_extra)

- **SubshellStart** → `__test_result = sash_fork();`. The existing Branch
  terminator then dispatches: child (pid 0 → false block) runs the bg body,
  parent (pid → true block) continues. Mirrors native emit_subshell_start.
- **StoreBgPid** → `$!` (the `!` var, `c_var_name` → `_c33`) gets the fork
  pid still sitting in `__test_result`, as a string.
- **Wait** → `sash_wait(sash_atoi(pid))`, adopting the child's status. A
  no-arg `wait` waits for `$!` (the `!` var) — this matters: `/bad & wait`
  must reap the failed child and exit nonzero. Mirrors native emit_wait.

## The 55.16 catch

First cut set no-arg `wait` to status 0, which **regressed 55.16**
(`/nonexistent & wait` → expects rc≠0). Before, `&` was a no-op so the
binary ran inline and failed; with a real fork the failure moves into the
child, so no-arg `wait` must reap `$!` (→ 127). Fixed by waiting for the `!`
var when present (else a bare success).

## Result

Full c sweep: **0 regressions, +2** (`c:38.5`, `c:55.19`) — and **zero
failing c tests remain**. The only remaining skipped c tests are
native-only-by-design (janet 15.x, native-ELF 32.x, CLI 57.46,
`$rerr`-message-only ones) plus the deferred `redirect-open-abort`
(55.24/25/26/30).

## Files

- `sash/src/emit_c.rs` — SubshellStart / StoreBgPid / Wait arms in
  `emit_builtin_extra`.
