title: "bug: multi-statement command substitution silently emits the literal placeholder $(...)"
status: done
priority: P2
kind: bug
depends: []
decision: "DG 2026-08-19 — IMPLEMENT (both multi-statement and pipeline bodies): run the full body in the forked child, capture stdout. Not reject."
resolved: "2026-08-19 — implemented native (c/janet capture is a pre-existing gap). Closed a coupled taint hole ($(-cmd) + first-stmt-only scan). Journal 154."
gates:
  - "`echo $(x=1; echo $x)` must execute (print `1`) — never the literal `$(...)`"
  - "`x=$(a | b)` (pipeline body — same code path) executes; its stages appear in `--manifest`"
  - "the `expr_to_string` `$(...)` / `$(( ... ))` debug arms are unreachable from codegen (assert/restructure)"

# Multi-statement / pipeline `$(…)` — DG: IMPLEMENT

## Decision (DG, 2026-08-19)

**Implement** — run the full body (multi-statement *and* pipeline) in the
forked child and capture its stdout, bash-faithful. Not reject. (`x=$(foo |
grep bar)` is bread-and-butter shell; rejecting would cripple usability, and
implementing unblocks `--manifest`.)

## Implementation plan (scoped 2026-08-19)

The single-command path is `handle_command_substitution` →
`SsaValue::CmdSubst { argv, redirects }` → `emit_cmdsub_exec` (fork, pipe,
exec one argv, parent reads the pipe into a string). The gap for a
multi-statement/pipeline body is a value that **captures a pipeline/block's
stdout to a string**. The pieces exist:

- `transform/control_flow.rs::build_pipeline_stages_from_cmdsubst` already
  turns a cmdsub body's statements into pipeline `stages` (reusable; used by
  `for x in $(cmd)` → `cmd | while read`).
- `SsaInst::Pipeline { stages, capture_read_fd }` runs stages and exposes a
  read fd; `emit_cmdsub_exec` (codegen, 192 lines) already does fork + pipe +
  parent-read-to-string for the single-argv case.
- `__compound_<entry>` forked stages (`stmt_handlers.rs::push_command_stage`)
  run an arbitrary lowered block in a forked child — the vehicle for a
  multi-*statement* (non-pipeline) body.

**Approach:** add an SSA value that carries pipeline `stages` (or a
`__compound_` block entry for a non-pipeline multi-statement body) and
captures the tail stdout to a string — e.g. `SsaValue::PipelineCapture {
stages }`, or generalize `CmdSubst` to hold either an `argv` or `stages`.
Then:

1. `handle_command_substitution`: when `single_command_subst` is `None`,
   classify the body — a single `Pipeline` → build stages; a multi-statement
   list → hoist into a `__compound_` block — and emit the capture value
   instead of the `SsaValue::Expr(expr_to_string(expr))` placeholder.
2. Codegen native (`emit_cmdsub_exec` sibling), `emit_c`, `emit_janet`:
   fork, pipe, run the stages/block in the child, parent reads to a string.
   Much of the native fork/pipe/read is reusable from `emit_cmdsub_exec`.
3. Make `expr_to_string`'s `$(...)`/`$(( ... ))` arms unreachable from
   codegen (assert), and audit every `SsaValue::Expr(expr_to_string(...))`
   fallback for other shapes that leak literal text.
4. Un-guard / strengthen `spec/undeclared.sh ud.16` (currently native-only,
   asserts only no-SB5031) to assert the runtime output.

**Size:** feature-sized, comparable to `c-janet-compound-pipeline-stage`;
real codegen risk across 3 backends. Warrants a focused session, not an
end-of-session add-on.

## ⚠ SECURITY: taint hole trap (found 2026-08-19)

A multi-statement/pipeline cmdsub captures **external** output → the result
is **tainted**. But two mechanisms currently assume clean:

- `validate.rs::cmdsub_is_taint_source` inspects only `stmts.first()`'s
  command name (`getenv`/`argv`/`arg`/`/abs`). For `$(a | b)` the first stmt
  is a `Pipeline` (not a `Command`) → returns `false`; for `$(x=1; cat f)`
  the first is an assignment → `false`. So the whole body is treated as a
  **clean** taint source.
- `ssa_taint.rs` `CaptureEnd` optimistically marks the target **Clean**
  ("assume a user function; the CST pass is the authority").

A naive capture reusing `CaptureStart/End` would let `$(external | cat)`
flow as **clean**, bypassing `bless` — a **validator bypass** (the #1 audit
priority). The implementation MUST:

1. Extend `cmdsub_is_taint_source` to scan **every** statement / pipeline
   stage for an external command (`/abs`) or `getenv`/`argv`/`arg` — not just
   `stmts.first()`.
2. Mark the SSA capture target **tainted** for a body-capture (a `tainted`
   flag on `CaptureEnd`, or a distinct inst), defense-in-depth vs the
   optimistic clean.
3. Add adversarial taint specs: `$(cat /etc/x | tr a b)` used unblessed →
   SB5020; a pure-clean body stays clean; blessed use compiles.

## Other correctness note: subshell isolation

`$(x=1; echo $x)` runs in a subshell in bash — `x` does not leak. The
`CaptureStart/End` mechanism runs the body **inline** (redirect stdout, no
fork), so a bare assignment would leak to the parent. `capture_func_into`
save/restores `local`s + positionals; a raw body needs save/restore of every
var it assigns (or a real fork) for subshell isolation. The captured *value*
is correct either way; only the side effect leaks.

## Reusable machinery (de-risked 2026-08-19)

- `functions.rs::lift_int_func_calls` — the hoisting pass (threads `block:
  &mut BlockId`), called at every arg + assignment-RHS site. Add a
  `CmdSubstBody` arm → `capture_body_into` → `Var(temp)`. Gives value-position
  coverage for free (value codegen has no `cl_blocks`; a statement-level
  capture inst does).
- `stmt_handlers.rs::capture_func_into` — the `CaptureStart` → lower-body →
  `CaptureEnd{target}` structure to mirror as `capture_body_into` (raw stmt
  list; no func-arg setup).
- `build_function_pipeline_stage` / `build_pipeline_stages_from_cmdsubst` —
  body→stages / body→compound-block precedent.
- `SsaInst::CaptureStart`/`CaptureEnd` codegen already exists — **no new
  codegen** for the inline-capture path.
- New transform-intermediate `SsaValue::CmdSubstBody(Vec<Statement>)` (ICE if
  it reaches codegen, like `IntFuncCall`); `handle_command_substitution`
  returns it for a non-single-command body.

## Original bug report

## Symptom

```
$ echo $(echo hi)          # single statement — works
hi
$ echo $(x=1; echo $x)     # multi-statement — WRONG
$(...)
$ echo $(true; echo done)  # multi-statement — WRONG
$(...)
```

A command substitution whose body is **more than one statement** is not
executed. Its value becomes the literal string `$(...)` at runtime. No
error, no diagnostic — a silent wrong result.

## Root cause

`transform/expr_handlers.rs::single_command_subst` returns `None` unless
the substitution body is exactly one statement (after stripping
`Comment` filler):

```rust
if real_stmts.len() != 1 {
    return None;
}
```

When it returns `None`, the value-lowering path falls through and the
expression is ultimately rendered by `transform/expressions.rs::expr_to_string`,
whose `CommandSubstitution` arm is a **debug placeholder**:

```rust
ExprKind::CommandSubstitution(_) => "$(...)".to_string(),
```

So a debug stringification leaks into real codegen output. The
single-statement fast paths (`expr_handlers.rs`, `control_flow.rs:452`
`stmts.len() == 1`, `stmt_handlers.rs:1000`) are the only ones wired to
actually execute a substitution.

## Decision needed (DG)

Two defensible resolutions — DG picks:

1. **Implement** multi-statement `$(…)`: lower the whole statement list
   into the forked child (capture its stdout), like a one-statement
   substitution but running N statements. This is the bash-faithful
   behavior.
2. **Reject** at validation: if the subset intentionally excludes
   compound command substitutions, emit a clear SB-diagnostic
   (e.g. "command substitution body must be a single command") instead
   of the silent placeholder.

Either way the silent `$(...)` placeholder must go. Whichever path is
chosen, the `expr_to_string` `"$(...)"` / `"$(( ... ))"` debug arms
should never be reachable from codegen — assert or restructure so a
stray placeholder is impossible.

## Notes

- Surfaced while adding DD#62 Model B specs (`spec/undeclared.sh`): the
  new SB5031 typo check correctly recurses into command-substitution
  bodies and validates their reads (subshell scope), which exposed that
  the *runtime* never executes multi-statement bodies.
- Re-confirmed 2026-07-29 during the `--manifest` fix (journal 134) via a
  **pipeline** body: `x=$(/bin/echo hi | /bin/cat)` → `got:$(...)`. Same
  root cause — `single_command_subst` returns `None` because the single
  statement is a `Pipeline`, not a `Command`. So the trigger is "body is
  not exactly one `Command`", which includes both multi-statement bodies
  *and* single-pipeline bodies.
- Dovetails with `--manifest`: once option 1 lands, the pipeline stages
  resolve through `resolve_pipeline_command` and auto-appear in the
  manifest (journal 134). Under option 2 they are rejected, mooting it.
- While fixing, audit **every** `SsaValue::Expr(expr_to_string(...))`
  fallback in `expr_handlers.rs` / `expressions.rs` for other shapes that
  reach codegen as literal text — the `"$(...)"` / `"$(( ... ))"` debug
  arms must be unreachable from codegen (assert or restructure).
- `spec/undeclared.sh` ud.16 is currently guarded to `native` and asserts
  only that the validator does not false-positive (no SB5031), NOT the
  runtime output, precisely because of this bug. Un-guard / strengthen it
  once this is resolved.
