# Journal 105 — scan_expr_positionals pair shrink

## Goal

Seventh Phase 11 target. The paired functions
`scan_expr_positionals_stmt` (CC 21) and
`scan_expr_positionals_expr` (CC 21) in
`transform/helpers.rs`. These compute, for a piece of code,
(a) the highest `$N` positional referenced and (b) the total
`shift` count, used to set the function-parameter prelude.

Audit recipe: extract `record_positional(name, max)` (used at 2
sites in expr) and `shift_amount(c) -> Option<usize>` (used at 1
site in stmt). Both small, both clarify the data flow.

## Action

Beyond the audited extractions, found two additional wins:

1. **`scan_stmts(stmts, max, shifts)`** — replaces the
   `for s in &stmts { scan_expr_positionals_stmt(s, max, shifts) }`
   pattern that appeared four times across both functions (For
   body, Case item bodies, default `_` `for body in
   child_bodies()`, and `CommandSubstitution` in expr). Single-
   line helper, but it elides four nested for-loops.

2. **Or-pattern merge for parts-vec variants.** `ExprKind::
   DoubleQuoted(parts) | ExprKind::Concat(parts) |
   ExprKind::Array(parts)` all wrap `Vec<Expr>` with identical
   walking behaviour. Verified by reading ir.rs:465/485/488 — all
   three are `Vec<Expr>`. Collapsed three arms into one. ir.rs
   already uses the same or-pattern at line 182 for DoubleQuoted |
   Concat so the precedent was set.

`shift_amount` itself absorbs three pieces of nested cleverness:
the `match &c.name.kind { Literal(s) => s.as_str(), _ => "" }`
fallthrough trick, the `name == "shift"` guard, and the
`args.first().and_then(|a| if let Literal { … })` closure. The new
helper uses early-return style — much easier to read.

## Verification

```
scan_expr_positionals_stmt   CC 21 → 14
scan_expr_positionals_expr   CC 21 → 10
shift_amount                       7   (new)
scan_test_positionals              6   (unchanged)
record_positional                  4   (new)
scan_stmts                         2   (new)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

Both `scan_expr_positionals_*` drop out of the top-5. New top-5:

```
49 check_stmt
43 validate_stmt_ctx
26 emit_test_expr   (emit_janet, deferred)
22 emit_command_kind (cyclo-allow'd, doesn't actually show)
…
```

(Filter view; the next live live entries after that are around CC
20.)

## Why this paired refactor was satisfying

These two functions are intricately co-recursive (stmt → expr →
stmt for CommandSubstitution). Trying to refactor either in
isolation would leave duplication unless you also moved into the
other. The `scan_stmts` helper threads both: it's called from
stmt's For/Case/default arms AND from expr's
CommandSubstitution arm. One helper, four formerly-duplicated
for-loops eliminated.

The audit estimated CC ~14 each. Actually got 14 + 10 — the expr
side benefited more than predicted because the or-pattern fold
on the three parts-vec variants wasn't in the audit notes (would
have required reading ir.rs to verify the shapes match, which
the audit didn't do).
