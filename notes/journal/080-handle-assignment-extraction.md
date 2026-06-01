# 080 — handle_assignment extraction

## Goal

Reduce `transform/stmt_handlers.rs::handle_assignment` (live CC 23;
audit 24) to a thin dispatcher. The audit called out one extraction
(`emit_capture_inline`, ~110 lines of FuncCall capture machinery);
the body has two more natural splits (subscript and append) worth
doing in the same pass.

## Approach

Three-way dispatch at the top:

```
if collision { diagnostic; }
if a.index.is_some()        → handle_subscript_assign
if a.op == Append           → handle_append_assign
else                         → handle_plain_assign
```

The `Append` arm itself dispatches on the value shape:
- `IndexedArray(elems)` → per-element `ArrayAppend`
- assoc array          → single `ArrayAppend`
- otherwise            → `append_scalar` (arithmetic add for int-
  typed vars, else `Concat`)

The `Plain` arm checks for `SsaValue::FuncCall(fname)` and delegates
to `try_capture_inline`, which returns `Some(restore_block)` when a
body is registered for `fname` and `None` otherwise. The Option lets
the caller fall through to the regular `Assign` path naturally.

The capture-inline body is then further decomposed into single-
purpose helpers (each `pub(self)` in the same module):

| helper | role |
| --- | --- |
| `save_positionals` | snapshot `$1..$9` + `$#` (returns refs only, no instructions) |
| `save_locals` | emit `__save_<fname>_<lname>` assigns |
| `extract_func_args` | pull the call args back out of the original `Expr` |
| `setup_func_args` | install `$#`, `$1..$N`, then clear `$(N+1)..$9` (DD#46) |
| `restore_positionals` | roll the saved positionals/locals back in |
| `check_var_func_collision` | DD#58 SB4019 diagnostic |

## Result

| function | CC |
| --- | --: |
| `handle_assignment` | 23 → **3** |
| `handle_subscript_assign` | 1 |
| `handle_append_assign` | 5 |
| `append_scalar` | 2 |
| `handle_plain_assign` | 4 |
| `try_capture_inline` | 2 |
| `save_positionals` | 2 |
| `save_locals` | 2 |
| `extract_func_args` | 5 |
| `setup_func_args` | 3 |
| `restore_positionals` | 3 |
| `check_var_func_collision` | 2 |

`handle_assignment` no longer appears in the file's top-10.

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. The two non-Append `IntFuncCall` lift +
  `Assign` lines were the original tail of the `else` arm; they
  move into `handle_plain_assign` after `try_capture_inline`
  declines.
- `try_X` Option pattern (used earlier this wave on `try_builtin` /
  `handle_command`) keeps the early-return / fall-through semantics
  intact without an explicit boolean flag.
- `extract_func_args` uses let-else (`let ExprKind::CommandSubstitution(stmts) = &value.kind else { return vec![]; };`)
  instead of a nested match — clippy-preferred since the early-out
  is a hard precondition.
- `save_positionals` exists mostly for readability; it issues no
  SSA instructions but it co-locates the snapshot logic with its
  symmetric `restore_positionals`.
