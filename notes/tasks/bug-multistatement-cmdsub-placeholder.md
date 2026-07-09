---
title: "bug: multi-statement command substitution silently emits the literal placeholder $(...)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`echo $(x=1; echo $x)` must either execute (print `1`) or be rejected at compile time — never silently print the literal string `$(...)`"
  - "decision needed (DG): implement multi-statement `$(…)` execution, or reject it at validation with a clear diagnostic"
---

# Multi-statement `$(…)` silently produces the placeholder `$(...)`

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
- `spec/undeclared.sh` ud.16 is currently guarded to `native` and asserts
  only that the validator does not false-positive (no SB5031), NOT the
  runtime output, precisely because of this bug. Un-guard / strengthen it
  once this is resolved.
