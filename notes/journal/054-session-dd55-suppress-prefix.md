---
title: "Session: DD#55 suppress prefix and error model gates"
date: 2026-05-11
---

## DD#55 Implementation — Suppress Prefix and Error Model

Full implementation of the `-` suppress prefix and completion of all
DD#55 error model spec gates.

### Grammar (tree-sitter-bash-safebash)

Added `suppressed_command` node to grammar, modeled after
`negated_command`. Supports:
- `- cmd` — bare suppressed command
- `- cmd1 | cmd2` — per-segment pipeline suppression (inside pipeline)

### IR

Added `StmtKind::Suppressed(Box<Statement>)` variant to the IR.
Wraps the inner statement — no need to modify every `Statement`
construction site.

### Lowerer

`lower_suppressed()` unwraps `suppressed_command` CST nodes into
`Suppressed(inner)`.

### Transform

Handles `Suppressed`: lowers inner statement normally, then marks
all emitted `Command`/`Pipeline` SSA instructions as `checked: false`
so codegen skips the implicit exit check.

### Codegen

No changes needed — `checked: false` was already implemented.

### $? scoping (dollar_question.rs)

Rewrote the rule with tri-state `HandlingCheck`:
- `NotHandling` — use normal $? rules
- `ValidFirst` — in handling position and $? is the first expression
- `StaleStatus` — in handling position but another command ran first

New error codes:
- **SB5013**: `$?` after `- cmd` — suppression discards exit status
- **SB5014**: `$?`/`$(status)` not first expression in handling branch

Rule now also catches `$(status)` (command_substitution containing
bare `status` command), not just `$?` (simple_expansion).

### $(status) / $(getcpid) lowering

Lowerer recognizes `$(status)` → `$?` and `$(getcpid)` → `$!` as
syntactic sugar. Command resolution rule allows `status`, `getcpid`,
`getpid`, `argc`, `signum` as special forms.

### || exit status fix

`||` RHS now resets exit status to 0 when the right side completes.
Previously, `/bin/false || s=hello` would exit 1 because the left
side's failure status leaked through.

### $(f) for -> int function (SB4018)

`$(f)` where `f` is `-> int` now emits a proper compile error instead
of an ICE. Error message: "cannot capture -> int function with $()
— use $((f))".

### Taint fixes

- CST taint: `extract_cmd_subst_name` and `cmdsub_args_clean` now
  unwrap `suppressed_command` to find the inner command.
- SSA taint: uses basename comparison (e.g., `/bin/echo` → `echo`)
  for known-clean command detection.

### Spec test fix

55.19: Changed `/bin/wait` to `wait` — `wait` is a shell builtin
with no `/bin/wait` binary.

### Gates resolved

| Gate | Description |
|------|-------------|
| 55.4 | `- cmd` suppresses exit check |
| 55.5 | `$?` after `- cmd` → SB5013 |
| 55.9 | `$(- cmd)` captures output, discards exit |
| 55.11 | `- cmd1 | cmd2` per-segment suppression |
| 55.12 | `$(f)` for `-> int` → SB4018 |
| 55.15 | `- cmd; return $?` → SB5013 |
| 55.17b | `$(status)` not first expression → SB5014 |
| 55.17c | `$(status)` as first expression → OK |
| 55.18 | `$?` in `||` RHS → valid handling position |
| 55.19 | `$!` sugar for `$(getcpid)` |

All 25 DD#55 spec gates now pass. Only 21.2 (pre-existing pipeline/
builtin issue) remains failing in errors.sh.

### Tests added

- `pos_suppress_prefix_false` — `- /bin/false; echo ok`
- `pos_suppress_pipeline_segment` — `- /bin/false | /bin/true`
- `neg_unsuppressed_false_aborts` — `/bin/false` without suppress aborts
