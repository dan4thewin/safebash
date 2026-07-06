---
title: "BUG — a user function used in a pipeline hits runtime `bug: bare command passed to sash_exec`; breaks the SB4021-4024 escape hatch"
status: done (native); c/janet split to c-janet-compound-pipeline-stage
priority: P1
kind: bug
depends: []
gates:
  - "`fn f { echo hi }; f | cat` runs and prints `hi` (function as pipeline producer)"
  - "`fn f { cat }; echo hi | f` runs (function as pipeline consumer)"
  - "`echo hi | f | cat` runs (function as a middle stage)"
  - "the SB4022 escape hatch works end-to-end: a compound wrapped in a function AND piped (`fn g { echo hi }; g | cat`) runs — no runtime `sash: bug:` message"
  - "function args pass through (`fn f { echo $1 }; f x | cat` — if args in pipeline position are supported); native + c + janet identical; specs added"
---

# BUG — functions don't work as pipeline stages (runtime `sash: bug:`)

Found 2026-07-04 (DG asked whether functions/groups in pipelines were
handled). Groups-as-consumer work (DD#14 compound stage); **functions
anywhere in a pipeline do not** — and they are the documented escape hatch
for the compound-postfix rejections, so this breaks that whole design.

## Repro (all compile clean, fail at runtime)

```
$ sash -c 'fn f { echo hi }
f | cat' -o /tmp/b; /tmp/b
sash: bug: bare command passed to sash_exec (expected absolute path)

$ sash -c 'fn f { cat }
echo hi | f' -o /tmp/b; /tmp/b        # consumer — same
$ sash -c 'fn f { cat }
echo hi | f | cat' -o /tmp/b; /tmp/b  # middle — same
```

The escape hatch the compiler itself recommends is broken:

```
$ sash -c '{ echo hi } | cat'                 # -> SB4022 "wrap in a function"
$ sash -c 'fn g { echo hi }
g | cat' -o /tmp/b; /tmp/b                     # the wrap -> runtime sash: bug:
```

## Why this matters

`compound_postfix_rejection` (`parse/parser.rs:181`) rejects a compound
command that is a pipeline producer / has a redirect / `&&`/`||`/`&`, with
**SB4021–SB4024**, each saying *"wrap in a function."*
`notes/tasks/converter-compound-postfix.md` codifies that idiom (the
converter will emit function wrappers). But a function pipeline stage
doesn't run — so the idiom, and any converted script that uses it,
produces a broken binary.

## Root cause

Pipeline stages are built in `handle_regular_pipeline`
(`transform/stmt_handlers.rs:684`) and `build_producer_stages`
(`stmt_handlers.rs:516`, `control_flow.rs:507`). A stage that is a
`StmtKind::Command` is passed through `resolve_pipeline_command`
(`transform/resolution.rs:6`), which returns the name unchanged for
anything that is not a `/`-path and not an inline builtin
(`echo|true|false|:`) — **including user functions** (it never consults
`state.known_functions`). Codegen then runs that name via
`lower_exec_stage` (`codegen/inst_handlers.rs:159`) → fork+exec of the bare
name → the `sash_exec` "bare command" backstop at runtime.

The `__compound_` pipeline path (`inst_handlers.rs:153`, DD#14) only
handles **non-`Command`** stages (a `{ }` group, etc.), so a function
*call* (a `Command`) never reaches it.

## Fix direction

Make a known-function `Command` stage lower like a compound: in the
pipeline stage builders, if `state.known_functions.contains(name)`, inline
the function body into its own SSA blocks (as the `__compound_` path does
for groups) and run it in the forked child, wired to the pipe fds — instead
of routing it through `resolve_pipeline_command`/exec. Cover:

- `handle_regular_pipeline` (general N-stage),
- `build_producer_stages` (the while-read / mapfile / lastpipe shapes —
  today they only handle `Command`/`Redirected(Command)` and silently drop
  other stages too),
- function **arguments** in pipeline position (the call may pass args — the
  inlined body must bind them),
- emit_c and emit_janet twins.

Rejecting functions-in-pipelines is **not** an option — it would break the
SB4021-4024 design. The fix must make them run.

## Relationship

- Unblocks the SB4021–4024 idiom and `converter-compound-postfix`.
- Sibling to the group-producer case (SB4022) — with this fixed, a group
  producer still errors but the recommended function wrapper works.
- The `sash_exec` "bare command" message is an internal backstop; the fix
  makes it unreachable from valid input (zero-tolerance: no runtime `bug:`
  from compilable source).
