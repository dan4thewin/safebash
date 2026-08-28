# 194 — remove dead `command` builtin codegen (DD#23 shadowed)

Continuing the coverage grind. The fully-uncovered-function scan flagged
`codegen/dispatch.rs::emit_command_builtin` (66/78 lines missed, the single
biggest cluster). Probing it: every `command …` form is rejected by the
validator with **SB4001** (`check_dynexec`, alongside `eval`/`source`/`.`).

DD#23 is explicit: *"No `command` builtin in sash-native — it's a coercion
target only"* (`command -v` becomes `[[ -x /path ]]`). The validator rejection
(commit `d455d98`) post-dates the codegen, so `emit_command_builtin` and the
whole `BuiltinCmd::CommandBuiltin` chain are dead-by-design vestige — the
pipeline stops at validation, never reaching transform/codegen.

## Removed

- `codegen/dispatch.rs`: the `BuiltinCmd::CommandBuiltin` dispatch arm + the
  `emit_command_builtin` method (~79 lines, a fork/exec path) + the now-unused
  `lower_exec` import.
- `ssa.rs`: the `CommandBuiltin` enum variant + its `from_name` (`"command" =>`)
  arm.

The SB4001 rejection in `validate.rs::check_dynexec` is **untouched** — that's
the security-relevant part and the user-visible behavior. Removing the dead
codegen also shrinks the command-execution attack surface (a fork/exec path
that no valid program could reach).

## Verification

`command /bin/echo hi` → still SB4001. Native gate 0 regressions. spec
coverage.sh cov.58 pins `command …` → SB4001 (covers the `check_dynexec`
`command` branch + guards the removal).
