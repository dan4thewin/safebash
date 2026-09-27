---
status: done
priority: P2
kind: bug
---

# native writes a redirect-failure diagnostic after restoring the fds

An **oracle divergence**: the two backends disagree, and native is the one that
diverges from bash. Found while probing the multi-redirect short-circuit
(journal 404); **pre-existing**, not caused by that change.

## The shape

When a command's redirect list contains a *successful* stderr redirect before a
*failing* open, bash emits the setup-failure diagnostic through the redirections
applied so far — i.e. into the file.

```sh
/bin/echo hi 2> capt.txt > /nonexistent_d/bad
```

| | stderr | `capt.txt` |
|---|---|---|
| **bash** | empty | `bash: /nonexistent_d/bad: No such file or directory` |
| **C backend** | empty | `./cbin: /nonexistent_d/bad: …` |
| **native** | `./q.bin: /nonexistent_d/bad: …` | empty |

C matches bash. Native does not.

## Cause

`sash/src/codegen/inst_handlers.rs::emit_redirect_guarded_body`, fail path:

```rust
emit_restore(saved.clone(), builder, module, ctx.externs);   // fds put back
if in_condition || emit_open_failure_abort(&rf, builder, module, ctx) {
```

`emit_open_failure_abort` is what calls `sash_emsg`, so by the time the message
is written stderr already points back at the terminal. The C backend emits its
`sash_emsg` before the restore, which is why it happens to be right.

## Why it is not a one-line move

`emit_open_failure_abort` does three things in one: writes the message, sets the
failing status, and *terminates the block* (jump to `end_cl`, or `exit` + trap).
Moving the restore after it would put the restore on an unreachable edge. The
fix has to split message-emission from the abort, with the restore between:

    emit message → restore → abort

and get the `ctx.in_end_block` case right (it returns `true` and falls through
to the merge rather than terminating, so the restore must still happen exactly
once on that path).

## Also check

`emit_redirect_push` (same file, the inlined-function redirect *scope*) has the
same message/restore question in cross-block form — DD#55's `checked` path.

## Gate

Needs specs in both modes. The existing `rd.*` specs in `spec/errors.sh` do not
reach it: rd.1 and rd.3 put the failing redirect first, so no stderr redirect is
in effect when the diagnostic is written. A spec for this must put a
*succeeding* `2>` before the failing open, and assert on the file's contents —
note that the harness's `$rerr` will be **empty** in the correct case, which is
exactly how the bug hid.

## Resolved 2026-09-27 (journal 425)

Fixed in both backends. `emit_open_failure_abort` split into
`emit_open_failure_msg` + `emit_open_failure_terminate`, with `emit_restore`
between them at both native sites (`emit_redirect_guarded_body` and
`emit_redirect_push`).

**The "also check" note was right, and bigger than expected.** The scope form
(`emit_redirect_push`) had the same inversion in the **C backend** as well —
so `emit_c.rs` held two functions doing the same job in opposite orders, and
the wrong one was paired against a native function that was also wrong.

That is why this half went unnoticed: **the differential oracle compares the
backends to each other.** Two identical wrong answers are indistinguishable
from two right ones. The command form was caught (journal 404) precisely
because the backends disagreed there; the scope form, one line away, did not
disagree. The oracle localises a bug to one backend but cannot see a bug in
the specification both were written from — only bash can.

Specs `rd.5`-`rd.8` in `errors.sh`, with `rd.6` as the ordering control.
Neuter: native inversion fails rd.5/rd.7/rd.8; C inversion fails rd.7 only,
which independently confirms C's command path was already correct.

Not a bug, checked before chasing: `cmd 2> capt > /bad || echo x` is silent
under sash where bash reports. DD#55 makes a caught failure silent
(55.25/55.50).
