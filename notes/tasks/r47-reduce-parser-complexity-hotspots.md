title: Reduce parser-complexity hotspots (CC > 50)
status: in-progress
priority: P1
kind: work
source: opus-47-review
branch: logos-lexer
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

## Progress (2026-05-27)

`next_token` migration landed. See `DD59-logos-structural-lexer.md`.

- `Scanner::next_token` CC: **78 → 15** (target was ≤50).
- Spec gates 285/285 unchanged; clippy clean; 12/12 lib unit tests.
- New `sash/src/parse/logos_tokens.rs` with 5 equivalence tests.
- Hand-roll boundary documented in the DD.

`tokenize_arith` split into per-class helpers (commit 23b1665).

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

1. ~~`tokenize_arith` (CC 88)~~ — **done** (commit 23b1665, 88 → 12).
2. ~~`next_token` (CC 78)~~ — **done** (DD#59, 78 → 15).
3. ~~`scan_double_quoted` (CC 68)~~ — **done** (68 → 9).
4. `intern_literals` (CC 70) — split by `SsaValue` variant. Today this
   is one giant match per variant; per-variant inline-able fns are
   cheaper to read.
5. `cli_main` (CC 69) — extract arg parsing, --extract, --janet, --ssa,
   --ir paths into named functions.
6. `parse_expr` (in arith.rs, CC 54) — extract the Pratt prefix and
   infix tables into static data; reduce the dispatch to two helpers.

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
