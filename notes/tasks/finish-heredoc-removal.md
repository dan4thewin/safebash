---
title: "finish removing heredoc support — RedirectOp::HereDoc is dead code"
status: done
priority: P3
kind: chore
depends: []
gates:
  - "RedirectOp::HereDoc is removed from the IR (ir.rs) and every match on it (codegen/redirects.rs, emit_c.rs, emit_janet.rs, validate.rs, ssa_taint.rs, convert.rs)"
  - "the lexer's `__HEREDOC__` sentinel + validate's SB5002 rejection remain the single path for `<<` / `<<-` (a clear 'heredocs not supported' error)"
  - "no behavior change: `<<` / `<<-` still rejected with SB5002; spec suite stays green"
---

# Finish removing heredoc support

Heredoc removal is **incomplete**. The lexer already rejects `<<` / `<<-`
by emitting a `__HEREDOC__` sentinel Word that `validate::check_heredoc`
turns into **SB5002** ("heredocs … are not supported — use herestrings
(<<<) or variables"). That is the intended, working rejection.

But the **`RedirectOp::HereDoc`** variant and all its handling survive as
**dead code** — the lexer never produces a `HereDoc` redirect, so these
arms are unreachable:

- `sash/src/ir.rs` — `RedirectOp::HereDoc` variant.
- `sash/src/codegen/redirects.rs` — fd-default + open/dup2 arms.
- `sash/src/emit_c.rs` — same.
- `sash/src/emit_janet.rs` — the `any`/`filter` matchers.
- `sash/src/validate.rs` — a `HereString | HereDoc` match.
- `sash/src/ssa_taint.rs`, `sash-convert/src/convert.rs` — references.

## Work
- Delete `RedirectOp::HereDoc` and collapse the `HereString | HereDoc`
  matches to `HereString` only.
- Confirm `<<` / `<<-` still rejected (SB5002) and `<<<` (herestring)
  unaffected.
- Spec suite must stay green (it's the source of truth).

## Discovered
2026-06-21 while verifying heredoc removal after DD#23 resolution work —
DG asked "check if heredoc support has been removed yet". Answer: rejected
at the lexer, but the IR/codegen machinery was never cleaned up.
