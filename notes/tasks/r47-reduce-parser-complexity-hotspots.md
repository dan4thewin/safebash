title: Reduce parser-complexity hotspots (CC > 50)
status: done
priority: P1
kind: work
source: https://github.com/dan4thewin/safebash/blob/f02477dd31e67a0fe6fe3c4a359935569c4715d8/notes/opus-47-review.md
branch: hand-parser
resolved: 2026-06-05
resolution: All 6 named hotspots landed (tokenize_arith 88→12, next_token 78→15, scan_double_quoted 68→9, intern_literals 70→6, cli_main 69→14, parse_expr 54→13). Transform/codegen follow-up wave (Phases 8-11) brought max non-allow'd CC from 49 → 26 (the live 26 is emit_janet, deferred to the SSA-rewrite task). All gates green at 291/291.
landed: 2026-06-05
depends:
  - r47-decide-logos-vs-handroll
gates:
  - spec 285/285
  - cargo test all suites pass
  - mk lint clean
  - top function CC ≤ 50 after change
  - DD written if logos migration succeeds

## Direction (DG, 2026-05-26)

Logos for the fixed-token layer (decision D1). Work happens on a new
branch `logos-lexer` off `hand-parser`. If the migration is clean and
gates stay green, write a DD capturing the choice (next in sequence
after DD58).

`scan_double_quoted` split into per-form helpers (commit pending).

- `parse/lexer.rs::scan_double_quoted` CC: **68 → 9**.
- Helpers: `scan_dq_escape`, `scan_dq_dollar`,
  `consume_dq_brace_expansion`, `consume_dq_arith_sub`,
  `consume_dq_cmd_sub`, `consume_dq_var_name`,
  `skip_balanced_squote`, `skip_balanced_dquote`,
  `scan_dq_backtick`. All ≤ 12.
- Spec gates 285/285 unchanged; clippy clean.

Top-10 after the three parser refactors:

```
70  intern_literals               sash/src/codegen/setup.rs:735
69  cli_main                      sash/src/lib.rs:70
63  emit                          sash/src/codegen/dispatch.rs:25
58  handle_param_expansion        sash/src/codegen/value_handlers.rs:18
57  lower_one                     sash/src/transform/statements.rs:27
54  parse_expr                    sash/src/parse/arith.rs:589
54  handle_pipeline               sash/src/transform/stmt_handlers.rs:264
48  to_tok                        sash/src/parse/logos_tokens.rs:136
48  print_arith_expr              sash/src/printer.rs:443
48  lower_value                   sash/src/codegen/values.rs:289
```

Only one parser function remains above 50: `parse_expr` (CC 54).
Everything else in the top-10 is codegen/transform.

`intern_literals` extracted to an `Interner` struct (commit pending).

- `codegen/setup.rs::intern_literals` CC: **70 → 6**.
- New `Interner<'a>` owns module borrow + literal map. Per-variant
  methods: `intern_value`, `intern_param_expansion`, `intern_ir_expr`,
  `intern_test_ir_expr`, `intern_test_expr`, `intern_inst`,
  `intern_trace_terminator`. All ≤ 17.
- Spec gates 285/285 unchanged; clippy clean.

`cli_main` split into mode-specific helpers (commit pending).

- `lib.rs::cli_main` CC: **69 → 14**.
- Helpers: `dump_sash_source` (--extract), `parse_cli_args`,
  `read_source`, `print_diags`, `emit_manifest`, `resolve_dep`,
  `compile_native`. All ≤ 13.
- Spec gates 285/285 unchanged; clippy clean.
- Max CC across the codebase: 69 → 63.

`parse_expr` Pratt loop extracted (commit pending).

- `parse/arith.rs::parse_expr` CC: **54 → 13**.
- Helpers: `try_postfix`, `try_assignment` (impl methods); free-fn
  tables `binary_op_for` (CC 21, declarative) and `assign_op_for`
  (CC 13, declarative).
- Spec gates 285/285 unchanged; clippy clean.
- Parser no longer appears in the top 10 hotspots.

## Progress (2026-05-27)

`next_token` migration landed. See `DD59-logos-structural-lexer.md`.

- `Scanner::next_token` CC: **78 → 15** (target was ≤50).
- Spec gates 285/285 unchanged; clippy clean; 12/12 lib unit tests.
- New `sash/src/parse/logos_tokens.rs` with 5 equivalence tests.
- Hand-roll boundary documented in the DD.

`tokenize_arith` split into per-class helpers (commit 353e0c6).

- `parse/arith.rs::tokenize_arith` CC: **88 → 12**.
- Helpers: `scan_arith_number`, `parse_arith_base_digits`,
  `scan_arith_dollar`, `scan_arith_ident`, `match_arith_op3`,
  `match_arith_op2`, `match_arith_op1`. All ≤ 21.
- Spec gates 285/285 unchanged; clippy clean.

`to_tok` at CC 48 is declarative (one arm per variant), so it
shouldn't count for review purposes — but flag it on the next pass
if we ever exceed the threshold by more than a token or two.

## Targets in this task

Pick functions in this order; each should land in its own commit with
the spec/test gates green:

1. ~~`tokenize_arith` (CC 88)~~ — **done** (commit 353e0c6, 88 → 12).
2. ~~`next_token` (CC 78)~~ — **done** (DD#59, 78 → 15).
3. ~~`scan_double_quoted` (CC 68)~~ — **done** (68 → 9).
4. ~~`intern_literals` (CC 70)~~ — **done** (70 → 6 via `Interner` struct).
5. ~~`cli_main` (CC 69)~~ — **done** (69 → 14).
6. ~~`parse_expr` (in arith.rs, CC 54)~~ — **done** (54 → 13).

All P1 parser/setup hotspots resolved. Remaining top-10 items are
transform/codegen-side. Open a follow-up task to lower those.

## Out of scope

- Anything already ≤50 — see `r47-set-extraction-floor-cc50`.
- Behavioral changes. This is pure restructuring; spec.db must stay
  285/285 after each commit.

## Why P1

These are the highest-leverage functions for code review and for the
adversarial-audit reviewers. They are also the densest source of
bugs-not-yet-found in any compiler. The `--top 10` list will not
change shape until these are addressed.

## Gate

`mk complexity` reports ≤50 for the top hotspot after the task is
done, and `mk check` is clean.
