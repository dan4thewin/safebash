# Journal 086 — emit_redirects shape-based extraction

## Goal

Reduce `emit_redirects` (`codegen/redirects.rs`) from CC 19 to floor.
A-verdict from `r47-abstraction-audit.md:166`. Audit recipe:
"'Literal-path-fast vs dynamic-path-slow' duplicated across
Write/Append/Read/HereString arms. Extract `emit_open_path(...)`."

## Result

| function                | CC |
| ----------------------- | -: |
| `emit_one_redirect`     |  5 |
| `open_flags_for_op`     |  5 |
| `default_fd_for_op`     |  4 |
| `emit_open_redirect`    |  3 |
| `emit_dup_output`       |  3 |
| `emit_redirects`        |  2 |
| `emit_heredoc_redirect` |  2 |
| `emit_save_fd`          |  1 |

Parent 19 → 2.

## Refactor shape

```
emit_redirects (loop body)
├─ default_fd_for_op(&op) -> u32          // 5-way RedirectOp → default fd
├─ emit_save_fd(fd, fd_val, ...) -> SavedFd
└─ emit_one_redirect(redir, fd_val, ...)  // dispatcher
    ├─ open_flags_for_op(&op) -> Option<i64>    // Write|Append|Read flag table
    ├─ emit_open_redirect(fd_val, target, flags, ...)  // 3 ops share this
    ├─ emit_dup_output(fd_val, target, ...)            // >&N
    └─ emit_heredoc_redirect(fd_val, body, ...)        // <<EOF / <<<"x"
```

## Key insight

The original code had three arms (Write|Append, Read, the literal-or-
dynamic body of HereDoc|HereString) that each open-coded the same
"literal path? use static literal; else compute ptr+len dynamically"
shape. The Write|Append arm packed flags 1/2 via a `matches!`, and
Read used flags 0 inline.

The collapse: factor flags into a small table (`open_flags_for_op`)
returning `Option<i64>`. The three open-and-dup2 ops (Write, Append,
Read) become a single call to `emit_open_redirect` after early-out on
`Some(flags)`. The match in `emit_one_redirect` drops from 5 arms to
3 (DupOutput / HereDoc|HereString / `_`).

Bonus: `default_fd_for_op` lifts the 5-way RedirectOp→fd table out of
the inline `unwrap_or(match ...)`, which both clarifies the loop body
and gives `emit_save_fd` something concrete to plumb.

## Refs vs values

`RedirectOp` doesn't derive `Copy` (it has `Clone` only). Without
`Copy`, `match redir.op { ... }` works for unit-only variants
(discriminant-only inspection) but `default_fd_for_op(redir.op)`
tries to move from `&Redirect`. Fix: helpers take `&RedirectOp`.
Could alternatively add `Copy` to `RedirectOp` (all 8 variants are
unit-like, so it's sound), but that's an unrelated IR change.

## Clippy

`emit_open_redirect` takes 8 args (fd_val, target, flags, builder,
module, externs, literals, vars) and trips
`clippy::too_many_arguments` (threshold 7). Added a function-level
`#[allow(clippy::too_many_arguments)]`. Bundling into a codegen ctx
struct would be a bigger blast-radius change than warranted by a
single function. (Already in this file: `emit_redirects` has 7 args,
`lower_ir_redirect_expr` has 6.)

## Behavior preservation

- Literal target not in literals map → silent skip (was per-arm
  `if let Some` with no else; now per-helper)
- HereDoc/HereString with literal-not-in-map → previously
  `continue` in the loop, now `return` from `emit_heredoc_redirect`.
  Same observable effect (the fd save already happened either way).
- DupOutput parse failure → defaults to fd 2 (unchanged)
- Unknown ops → no-op after save (unchanged)

## Gates

- `cargo build --release` ok
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/codegen/redirects.rs` — replaced the 130-line `emit_redirects`
  loop body with the 8-helper tree above.
- `notes/tasks/r47-abstraction-audit.md:166` — annotated.
