# Journal 085 — handle_read flag-parser extraction

## Goal

Reduce `handle_read` (`transform/cmd_handlers.rs`) from CC 21 to floor.
A-verdict from `r47-abstraction-audit.md` (line 224). Audit recipe:
"Hand-rolled flag-parser with awkward `skip_next` state. Extract
`parse_read_flags` returning (delim, field_sep, array, vars)."

## Result

| function            | CC |
| ------------------- | -: |
| `parse_read_flags`  | 10 |
| `emit_read_inst`    |  6 |
| `handle_read`       |  4 |
| `consume_byte_flag` |  3 |

Parent CC 21 → 4. Total 23 across 4 functions (slightly higher than the
single-function 21 because the dispatcher in `emit_read_inst` re-counts
the `0 / 1 / many` arms, but each piece is independently legible).

## Refactor shape

```
handle_read
├─ parse_read_flags(args, stmt, state) -> Option<ReadFlags>
│   └─ consume_byte_flag(arg, slot, args, i) -> bool   // -d/-f shared
└─ emit_read_inst(flags, ssa, state, block)            // ReadArray / ReadLine / ReadLineMulti
```

`ReadFlags` is a private struct (`{ delim, field_sep, array_name, var_names }`).
`var_names` borrows from the caller's `args: Vec<String>` (no clones).

`parse_read_flags` returns `Option<ReadFlags>` — `None` means a
diagnostic was emitted (SB5015 unknown flag); parent early-outs.

`consume_byte_flag` handles the joined (`-dX`) / split (`-d X`) duality
shared by `-d` and `-f`, returning `true` when it consumed a following
arg so the caller advances `skip_next`. Drops the parallel hand-rolled
`if a.len() > 2 ... else if i + 1 < args.len() ...` blocks each flag had.

The SB5016 check (field separator without enough vars) stays in the
parent — it depends on cross-field state (`field_sep.is_some() &&
var_names.len() <= 1 && array_name.is_none()`) which is cleaner to
read at the top level than threaded through the parser.

## Note: two `parse_read_flags` in the tree

`transform/stmt_handlers.rs:418` already has a `parse_read_flags`
returning `(Vec<String>, Option<u8>, Option<u8>)` used only by
`handle_lastpipe_read` (DD#20 lastpipe). Its semantics differ:

- No `-a` support.
- Unknown flags silently skipped (lastpipe trusts the outer `read`
  was already validated).
- `-d` with no following arg sets `delim = Some(0)`; this version
  leaves it `None`.

These are not interchangeable. Both are private (`fn`, not `pub`); no
collision. Future cleanup could rename one for clarity but the audit
recipe didn't call for unification, and the semantic divergence is
real.

## Gates

- `cargo build --release` ok
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/transform/cmd_handlers.rs` — replaced 95 lines of
  hand-rolled flag parsing with the 4-function tree above.
- `notes/tasks/r47-abstraction-audit.md` — annotated line 224.
