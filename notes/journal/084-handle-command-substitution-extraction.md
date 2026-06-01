# 084 — handle_command_substitution extraction

## Goal

Finish off `transform/expr_handlers.rs::handle_command_substitution`
(live CC 21; audit 37 — Phase 8 BuiltinCmd already trimmed 37 → 21
via the typed dispatcher). The remaining complexity was deep
nesting from shape probing (`CommandSubstitution` → filter comments
→ single-stmt check → unwrap wrappers → `Command` extract → literal-
name check) and the inline builtin dispatch.

## Approach

Linear top-level after extraction:

```rust
let Some((stmt, name_opt, c)) = single_command_subst(expr) else {
    return SsaValue::Expr(expr_to_string(expr));
};
if let Some(name) = name_opt {
    if state.known_functions.contains(name.as_str()) {
        return SsaValue::FuncCall(name);
    }
    if let Some(v) = try_native_intrinsic(&name, c, state) { return v; }
}
let argv = build_argv(c, state);
let redirects = match &stmt.kind {
    StmtKind::Redirected(r) => r.redirects.clone(),
    _ => vec![],
};
SsaValue::CmdSubst { argv, redirects }
```

Helpers:

| helper | role |
| --- | --- |
| `single_command_subst` | Probe shape; returns `(outer Statement, name?, Command)` or `None`. Outer statement is preserved so redirects can be recovered from the wrapper, matching pre-refactor behaviour. |
| `unwrap_subst_wrappers` | Peel `Redirected` / `Suppressed` off a `StmtKind`. The original wrote the same loop inline with an unused `_is_suppressed` flag. |
| `try_native_intrinsic` | `BuiltinCmd::from_name` dispatch over Len/Keys/Join/Echo, returning `None` so the caller can fall through. |
| `build_argv` | head + args lowered with `expr_to_value`. |

`try_len_intrinsic`, `try_keys_intrinsic`, `join_intrinsic`,
`echo_concat` already existed and are unchanged.

## Result

| function | CC |
| --- | --: |
| `handle_command_substitution` | 21 → **6** |
| `try_native_intrinsic` | 6 |
| `single_command_subst` | 5 |
| `unwrap_subst_wrappers` | 5 |
| `build_argv` | 2 |

Top of file is now dominated by the arithmetic-folding helpers
(`fold_arith_op` 25, `ir_op_to_ssa` 20), not the command-subst
handler — exactly the inversion the audit asked for.

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. The `_is_suppressed` flag in the original
  was computed but never read; `unwrap_subst_wrappers` drops it
  cleanly.
- `single_command_subst` returns the outer `&Statement` (not the
  inner one) because redirect capture happens at the wrapper level
  in the original — preserving the same semantics for `$(cmd > f)`.
- The cascade of let-else / if-let in the parent matches the
  recent wave-2 style (parse_command_substitution, handle_assignment)
  for consistency.
- Audit's "per-builtin `try_X` helpers" recipe was already partially
  realised by Phase 8's `try_len_intrinsic`/`try_keys_intrinsic`;
  `try_native_intrinsic` is the new top-level table that ties them
  together with the `BuiltinCmd` enum from DD#56.
