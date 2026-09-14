---
title: 317 — The machinery nothing drove
---

DG: *drop the unreachable compound code.*

`BuiltinCmd::RedirectPush` / `RedirectPop` were a matched pair: push
saves the affected fds and applies a redirect, pop restores them. They
existed so a *compound* statement — a group, a `for`, a `while` — could
carry a redirect across its whole body rather than around a single
command. Both backends implemented them. Both backends maintained a
saved-fd **stack**, because compounds nest.

Nothing ever emitted them.

The only producer was `bracket_compound_with_redirects` in
`transform/statements.rs`, reached from `lower_redirected_stmt` when
`is_compound_stmt` said yes. But SB4021 rejects a redirect on a group or
a loop in the parser, long before transform runs. The `needs_bracket`
branch was a road with no on-ramp.

So this was 141 lines spread over six files, two independent
implementations of the same protocol, kept mutually consistent across
however many refactors — for a case the front end forbids.

## What came out

| file | removed |
|---|---|
| `transform/statements.rs` | `is_compound_stmt`, `bracket_compound_with_redirects`, the `needs_bracket` branch |
| `ssa.rs` | both `BuiltinCmd` variants, their `from_name` arms, their `sigil_str` arms |
| `codegen/dispatch.rs` | both dispatch arms, `emit_redirect_push`, `emit_redirect_pop` |
| `codegen/types.rs` | `InstCtx::redirect_stack` |
| `codegen/entry.rs` | the local stack and its threading through `emit_block_insts` |
| `emit_c.rs` | `Emit::redirect_stack`, `emit_compound_redirect`, its interception in `emit_command_body`, two exhaustive-match arms |

The `ice_unreachable_command` arm is worth calling out. It listed
`Write | RedirectPush | RedirectPop` together under one comment —
"rewritten before codegen". That was true of `Write` (DD#99 turns it
into a newline-suppressed `echo`) and *technically* true of the other
two in the sense that they were intercepted earlier, but the honest
statement was that they never existed at all. The arm is now just
`Write`, and the comment says only what is true.

## The tell

`emit_c.rs` carried a doc comment reading "a `mapfile -t a < file` wraps
the read inst in a push/pop pair". `mapfile` is not a sash builtin — it
was retired. The comment outlived the feature that motivated it, and
then the mechanism outlived the comment. Dead code does not announce
itself; it accretes plausible-looking justification.

Sweep unchanged: 4691 passing, 0 failing. No spec moved, which is the
point — there was nothing to move.
