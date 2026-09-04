---
title: "close N builtin — the sanctioned fd-close form (DD#14; SB4052)"
date: 2026-09-04
---

# `close N` builtin (DD#14 fd-manipulation)

DG: "this gives SB4048: exec 9> foo; exec 9>&-".

## Not a bug — a deliberate gap

`exec` fd-manipulation (DD#14) supports **open** (`exec 9> foo`, `exec 3< file`)
and **dup** (`exec 2>&1`, `N>&M`), but the **close** form `N>&-` is rejected
(SB4048). That reject was deliberate: the `bug-redirect-dup-target-unvalidated`
fix (2026-08-29) closed a silent-degrade hole (`>&` with a bad target used to
dup to fd 2) reject-only, leaving fd-close unimplemented — tracked in the
pending `fd-manipulation-builtins` decision. sash had no way to close an fd.

## DG decision

Presented the fork (builtin vs `N>&-` syntax vs both). DG chose **A, close-only**:
a `close N` builtin, void, DD#55-abort on failure, coexisting with the existing
open/dup syntax; dup stays on `N>&M`. This matches sash's "named builtins over
terse punctuation" direction (DD#65/#75) and doesn't reopen the `>&`-target
parsing the 2026-08-29 fix hardened.

## Implementation (native)

`close` is an ordinary void builtin, modeled on `cd`:

- `ssa.rs`: `BuiltinCmd::Close` + `"close" => Close` in `from_name`. It then
  falls through the transform's `try_builtin` (`_ => None`) like `cd`/`exec` and
  is emitted as a generic `SsaInst::Command { kind: Close, checked }`.
- `validate.rs`: `close` in `BUILTINS`; `check_close_arg` requires exactly one
  **literal non-negative fd number** (a variable/expansion or bad arity →
  **SB4052**, the same no-dynamic-fd discipline as a `N>&M` dup target).
- `codegen/dispatch.rs` + `inst_handlers.rs`: `handle_close` — `lower_value_as_int`
  the fd, call `sash_close`, set `$?`/`test_result` from the 0/-1 result, then
  `emit_status_abort_check` (DD#55: bare aborts, caught/`-` doesn't) with a
  `close: <fd>: <reason>` diagnostic.
- `sash-rt`: `sash_close` already existed but collapsed the syscall error to
  `-1` without recording the errno, so a `close` abort printed `error 0`. Added
  `set_last_errno_raw` and set `SASH_LAST_ERRNO = -result` on failure → the
  abort now renders the real reason (EBADF → "Bad file descriptor").
- `parse/parser.rs`: the SB4048 message now points at `close N`.

## Behavior

| input                                  | result                                    |
|----------------------------------------|-------------------------------------------|
| `exec 9> f; /bin/echo hi >&9; close 9` | ok — file gets `hi`, fd closed            |
| `close 9` (unopened)                   | DD#55 abort `close: 9: Bad file descriptor`, rc 1 |
| `if close 9 { … } else { … }`          | caught — else branch, no abort            |
| `- close 9`                            | suppressed — no abort                     |
| `close` / `close foo` / `close 1 2`    | **SB4052**                                |
| `exec 9>&-`                            | still **SB4048** (→ use `close N`)        |

## Backends

Native-only, like `exec`. c no-ops the `close` TODO; janet mis-emits it as an
external `os/execute ["close"]` (both consistent with how they treat
`exec`/`cd`). Folded into `c-janet-exec-support`. `spec/exec.sh` is already
native-guarded — the natural home.

## Verification

- `spec/exec.sh` cl.1–cl.8 (native): open+close, DD#55 abort/caught/suppressed,
  SB4052 arity/non-fd, SB4048-still-rejects. plan 11→19.
- Full 3-mode sweep, latest-run diff vs HEAD (`a0a775f`): **0 regressions**, 8
  new passing (native cl.*), 0 gone.
- `cargo fmt`/`clippy` clean on sash; sash-rt warnings all pre-existing.

DD#14 amended (`close` section); `fd-manipulation-builtins` resolved (done);
`c-janet-exec-support` gained a close gate. SB4052 is a new code.
