---
title: "bug: $(printf …) and other abs-path builtins not resolved inside command substitution → bare command passed to sash_exec"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "$(printf '%s' x) resolves printf to its absolute path (like a direct `printf` command does) instead of passing the bare name to sash_exec"
  - "the fix matches direct-command resolution (DD#23 resolve_pipeline_command / BuiltinCmd path-resolution) so cmdsubst and direct commands agree"
  - "spec test: a command substitution capturing an abs-path builtin (printf) produces the expected output in native + c"
---

# `$(printf …)` passes a bare command name to sash_exec

## Symptom

A command substitution whose command is an absolute-path builtin (e.g.
`printf`) emits a **bare** `argv[0]` to `sash_exec`, which rejects it:

```
x=$(printf '%s-' one two three)   # → sash: bug: bare command passed to
                                   #     sash_exec (expected absolute path)
```

Confirmed 2026-06-21 on **committed** code (stash-verified), so this is
**pre-existing** and unrelated to the array-splat work that surfaced it.
A direct `printf '%s-' one two three` resolves fine to `/usr/bin/printf`;
only the `$( )` path leaves it bare.

## Root cause (suspected)

The cmdsubst argv head is built by `build_argv` in
`transform/expr_handlers.rs` via `expr_to_value(&c.name)`, which yields a
bare `Literal("printf")`. The direct-command path resolves the name with
`resolve_pipeline_command` / `BuiltinCmd` absolute-path resolution
(DD#23); the cmdsubst path does **not** apply the same resolution to its
head, so a builtin-resolved-to-abs-path name like `printf` stays bare.

## Fix

Apply the same command-name resolution to the cmdsubst head that direct
commands and pipeline stages use (`resolve_pipeline_command`). Externals
that already carry a slash are unaffected; the gap is the abs-path
builtins (printf, …).

## Notes

- Surfaced while implementing `array-splat-command-arg`: the cmdsubst
  splat path (`$(/usr/bin/printf '%s-' @a)` with an explicit abs path)
  works correctly once the head resolves — verified output `one-two-three-`.
- External commands (true externals with a `/`) are unaffected.
