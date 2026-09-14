---
title: "Remove the dead compound-redirect machinery (RedirectPush/RedirectPop)"
status: blocked
priority: P3
kind: cleanup
depends: []
gates:
  - "`BuiltinCmd::RedirectPush` / `RedirectPop` no longer exist, or reaching them ICEs"
  - "sweep stays at 0 failing"
---

# Dead: the compound-redirect machinery

**Blocked on DG** — this is a removal of a deliberately-built feature
implementation, so it needs a decision, not a patch. See journal 307.

## The finding

`transform/statements.rs:bracket_compound_with_redirects` is the **only**
producer of `BuiltinCmd::RedirectPush` / `RedirectPop`, and it cannot run.

- `StmtKind::Redirected` has exactly two construction sites, both in the
  parser: `parse_command` (which wraps a `StmtKind::Command`) and
  `wrap_with_redirects`, reached only from `wrap_pipeline_part`.
- `parse_statement` and `parse_list_operand` both branch
  `if is_compound(&stmt.kind) { reject_compound_postfix(…) } else { …redirects… }`,
  so a compound command never reaches the redirect-wrapping side. It gets
  **SB4021** instead.
- The one `StmtKind` in transform's `is_compound_stmt` but not the parser's
  `is_compound` is `Function`, and a redirect on a function definition is
  **SB4046** (`noncommand_redirect_target`).

Therefore `is_compound_stmt(&r.body.kind)` in `lower_redirected_stmt` is
always false. Measured, not inferred — `{ echo a; } > f`,
`while …; done > f` and `if …; fi > f` all fail with SB4021.

## What is dead as a consequence

- `transform/statements.rs` — `bracket_compound_with_redirects`,
  `is_compound_stmt`, the `needs_bracket` branch of `lower_redirected_stmt`
- `codegen/dispatch.rs` — `emit_redirect_push`, `emit_redirect_pop`, and
  their two arms in `emit_command_kind`
- `emit_c.rs` — `Emit.redirect_stack`, `emit_compound_redirect`, its
  interception in `emit_command`, and the `RedirectPush | RedirectPop` arm of
  `ice_unreachable_command`
- `ssa.rs` — the `RedirectPush` / `RedirectPop` variants plus their
  `from_name` and `sigil_str` arms

## Why it is dead now but was not always

Journal 160 implemented the emit_c half for `mapfile -t a < file`, which
lowered to a `RedirectPush … Mapfile … RedirectPop` triple. Journal 268 gave
`ReadArray` its own redirect list — so it could carry the DD#55 open(2) guard
— and that removed the last real producer. Journal 281's ICE-arm trace
recorded the pair as "intercepted by `emit_compound_redirect`", which was true
when written and is now only true vacuously.

## The decision

**For removal:** SB4021 is deliberate, and the direction of travel reinforces
it — task `converter-compound-postfix` has `sash-convert` rewriting compounds
into functions precisely so the rejection can stand. Dead code that spans four
modules is a standing invitation to mis-trace (journal 281 already did).

**Against removal:** a future DD could allow compound redirects, and this is a
working implementation of them in both backends. Deleting it means rewriting
it later.

**Middle option:** keep the SSA variants but make the transform arm ICE, so
the dead path is *documented as impossible* rather than silently present.
