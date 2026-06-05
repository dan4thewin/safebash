# Journal 106 — check_stmt extraction

## Goal

Eighth Phase 11 target. `check_stmt`
(`type_check.rs:243`, CC 49). The audit verdict was "biggest lift":
nine arms, including 4-block recursion in `If`, a 10-arm `Command`
dispatch with two builtin special cases, and a non-trivial
`Declaration` arm.

The journal 056 warning is load-bearing: a previous attempt
collapsed everything into `_ => child_bodies()` and silently
skipped type checking inside Pipeline/Group/Subshell/List/
Redirected wrappers. Four oracle tests were added to catch
exactly that regression. Whatever I did had to preserve those.

## Action

Extracted per-arm helpers for every arm with non-trivial pre-work
or branching:

- `check_assignment(a, span, env, errors)` — compatibility check
  vs already-declared type, declare on first use.
- `check_declaration(d, span, env, errors)` — per-assignment
  Int/String cross-type check, collection tracking.
- `check_command(c, span, env, errors)` — top-level dispatch on
  cmd_name + DD#33 subscript-type loop, delegates to:
  - `check_unset_args` (DD#30, the "unset on scalar is rejected"
    rule; restructured with early-continue style instead of 4-deep
    nesting).
  - `check_exit_arg` (`exit $intvar` clamp suggestion; also
    early-return cleaned up).
- `check_if(if_stmt, env, errors)` — preserves the
  `check_condition_type` pre-work, then walks 4 body sets.

Folded For, While, Function, Case into the existing default `_
=> child_bodies()` arm. Verified one-by-one against `ir::
child_bodies()` (ir.rs:82) that the slices returned match the
fields each explicit arm was walking:

| arm | explicit walk | child_bodies() returns |
|---|---|---|
| For | `&for_stmt.body` | `vec![&f.body]` |
| While | `&while.condition`, `&while.body` | `vec![&w.condition, &w.body]` |
| Function | `&f.body` | `vec![&f.body]` |
| Case | `for item in items { &item.body }` | `c.items.iter().map(\|i\| &*i.body).collect()` |

All equivalent. The default arm already descended through these
once the journal 056 fix was in place, so the explicit arms were
strictly redundant.

## Why If stays explicit

`check_if` calls `check_condition_type(&if_stmt.condition, …)`
BEFORE recursing — that pre-work would be lost if I folded it
into the default. While *could* benefit from the same check
(int-function-in-boolean-context applies to `while (( … ))` too),
but that's a separate behaviour question, not a refactor.

## Verification

```
check_stmt                  CC 49 → 8
check_declaration                 11  (new)
check_if                          8   (new)
check_unset_args                  7   (new)
check_command                     6   (new)
check_assignment                  4   (new)
check_exit_arg                    4   (new)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

`check_stmt` drops out of the top-5. New top-5:

```
43 validate_stmt_ctx   ← next
26 emit_test_expr      (deferred)
19 infer_stmt          (type_check.rs:?? — surfaced from #1 drop)
17 scan_value_for_vars
16 parse_command_substitution
```

## Why this came out smaller than the audit estimate

The audit predicted CC ~12. Got 8. Two reasons:

1. The "fold four trivial walkers into default" insight wasn't in
   the audit notes — it required reading `child_bodies()`
   carefully to confirm semantic equivalence. Eliminated four
   complete arms at once.
2. Restructuring `check_unset_args` from 4-deep nested ifs to
   early-continue dropped two extra CC points off the helper's
   own count.

The journal 056 bug-fix that kept the default arm alive is
exactly what made this refactor safe AND profitable.
