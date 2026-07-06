# 130 — functions as pipeline stages (bug-function-in-pipeline, native)

P1 bug (DG picked it as the next batch): a user function anywhere in a
pipeline compiled clean but crashed at runtime with `sash: bug: bare
command passed to sash_exec`. This broke the **SB4021-4024 escape hatch** —
the compiler itself tells you to "wrap in a function" when a compound can't
be a pipeline stage, but the wrap then didn't run.

## Root cause

`handle_regular_pipeline` (and `build_producer_stages` for the
pipe-while-read / lastpipe shapes) routed a `Command` stage through
`resolve_pipeline_command`, which returns a user-function name unchanged
(it only maps `/`-paths and the inline builtins). Codegen then fork+exec'd
the bare name → the `sash_exec` "bare command" backstop. The DD#14
`__compound_<id>` mechanism (for `{ }`/for/if stages) only handled
**non-`Command`** stages, so a function *call* never reached it.

## Fix (native)

Make a known-function `Command` stage lower like a compound: inline the
body into its own SSA blocks (a forked child), bind the call args as
$1..$N, and name the stage `__compound_<entry>` — which native codegen
already forks and runs. New `build_function_pipeline_stage`:

- `resolve_function_name` + `function_bodies` to get the body; DD#60
  `expand_dollar_at_in_body` so `$@` becomes the explicit positionals.
- `setup_callee_positionals` binds $1..$N/$# from the call args (reused
  from the normal-call path; made `pub(super)`).
- a `ret_block` as the `return_target` so `return` ends the forked child;
  body lowered via `lower_stmts`, exit → ret_block → `Return`.
- **subshell semantics** via `state.snapshot()` / `state.restore()` around
  the inline: the stage's variable assignments (positionals, locals, and
  globals) don't leak to the parent's version map — the same mechanism
  control_flow uses for branches. (pf.9 asserts `x=child` inside a piped
  function doesn't change the parent's `x`.)

Both the regular pipeline and the producer-stage builder share a new
`push_command_stage` helper (function-inline vs exec decision) — which also
kept both dispatchers under the cognitive-complexity gate.

## c/janet: pre-existing gap, split out

DG-directed scope decision. c and janet have **no `__compound_` handling at
all** — even a plain `{ }`/for/if pipeline stage fails there (c: same
`bare command` bug; janet: `unknown symbol p1`). So functions-in-pipeline
for c/janet is part of a larger missing DD#14 backend, filed as
`c-janet-compound-pipeline-stage` (P2). `spec/pipe_functions.sh` is
native-only guarded (like `brace.sh` / `convert.sh`).

## Verification

- `spec/pipe_functions.sh` (native) pf.1-11: producer, consumer, middle,
  1/2 args, the SB4022 multi-statement wrap, global read in stage, both-
  ends functions, subshell no-leak, real consumer (`grep`), and the
  function-producer → while-read (`build_producer_stages`) path.
- Full spec.db diff: **native 0 regressions** (c/janet unchanged — the
  fix is native-only and the spec is guarded). Complexity gate green.

## Files

- `sash/src/transform/stmt_handlers.rs` — `build_function_pipeline_stage`,
  `push_command_stage`, `handle_regular_pipeline` + `build_producer_stages`
  (thread `ssa`, function branch).
- `sash/src/transform/cmd_handlers.rs` — `resolve_function_name` /
  `setup_callee_positionals` made `pub(super)`.
- `spec/pipe_functions.sh` (new, native-guarded).
