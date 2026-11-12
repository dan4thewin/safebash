title: "bug: $(printf …) and other abs-path builtins not resolved inside command substitution → bare command passed to sash_exec"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "$(printf '%s' x) resolves printf to its absolute path (like a direct `printf` command does) instead of passing the bare name to sash_exec"
  - "the fix matches direct-command resolution (DD#23 resolve_pipeline_command / BuiltinCmd path-resolution) so cmdsubst and direct commands agree"
  - "spec test: a command substitution capturing an abs-path builtin (printf) produces the expected output in native + c"

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

## Resolved (2026-06-21)

`build_argv` (expr_handlers.rs) now resolves the cmdsub head via
`resolve_pipeline_command` (DD#23), like direct commands / pipeline
stages. Bare external heads (`cat`, `printf`, `grep`, …) resolve to
absolute paths; `getenv`/`argv`/`arg`/`echo`/user-functions/abs-paths are
unchanged. Taint is correctly enforced by the SSA-level `cmd_subst_taint`
(external → tainted, pure builtins like printf → clean, user-fn → clean).
Rust `cmdsub_with_args`/`cmdsub_empty_output` now pass; spec commands 14.2b
(native). Native baseline held.

Surfaced (filed): (1) an **unblessed** bare-external cmdsub trips the SSA
ICE backstop ("CST taint pass missed this") instead of a clean SB5020 —
pre-existing; the CST taint pass under-classifies bare externals. (2)
`getpid`/`getppid`/`getcwd`/`epochseconds`/`random`/`argc`/… intrinsics
aren't intercepted in cmdsub (only getenv/argv/arg are) → "bare command".
(3) The **C** backend's cmdsub exec is an unimplemented Phase-4 TODO.
