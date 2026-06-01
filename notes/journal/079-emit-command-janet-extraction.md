# 079 — emit_command (Janet) extraction

## Goal

Reduce `emit_janet.rs::emit_command` (live CC 24; audit 39) to the
BuiltinCmd-dispatcher floor. Phase 8's BuiltinCmd refactor already
clipped 39 → 24; this finishes by lifting every multi-line arm into
a per-builtin helper.

## Approach

Six lifts plus one merge:

1. `emit_janet_const_bool(out, success)` — folds `True` and `False`
   into one helper. Each writes `(set __last_exit N)` + the literal.
2. `Break | Continue` — merged into one arm; Janet's `(each)` lacks
   true continue, so both already emitted `(break)`. The TODO about
   restructuring with `when`/`unless` is preserved as a comment.
3. `emit_janet_test_cmd(out, ctx, c)` — Test-arg-attached `[[ … ]]`.
4. `emit_janet_return(out, ctx, c)` — `return [code]` form.
5. `emit_janet_bless(out, c)` — DD#26 bless validation (commented in
   Janet backend).
6. `emit_janet_unset(out, ctx, c)` — `unset var…` loop.
7. `emit_janet_general_command(out, ctx, c, name)` — the `_ =>` arm:
   known function call vs `os/execute`.

The dispatcher itself is now one-line-per-arm and tagged
`// cyclo-allow:` — 14 arms by construction.

## Result

| function | CC |
| --- | --: |
| `emit_command` | 24 → **15** (cyclo-allow'd) |
| `emit_janet_general_command` | 4 |
| `emit_janet_unset` | 3 |
| `emit_janet_const_bool` | 2 |
| `emit_janet_test_cmd` | 2 |
| `emit_janet_return` | 2 |
| `emit_janet_bless` | 2 |

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. Break/Continue merge is exact (both emitted
  the same `(break)`).
- `emit_janet_const_bool` takes a `bool` parameter; the body picks
  the (exit, literal) pair via destructuring — keeps both arms in
  one place.
- Other Janet emit helpers (`emit_janet_echo`, `emit_janet_exit`,
  `emit_janet_printf`, `emit_janet_read`, `emit_janet_cat`) already
  existed pre-refactor; only new helpers are listed above.
