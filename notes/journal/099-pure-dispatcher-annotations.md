# Journal 099 — annotate pure-dispatcher hotspots

## Context

Post-Phase-10 audit of remaining top-15 cyclomatic-complexity
hotspots (those not already `// cyclo-allow:` tagged) classified the
candidates into three buckets:

- **A (extract)** — recipes apply (taint_stmt j093 family,
  lower_test_binary j097 family, ir_to_ssa_typed phase split, etc.).
  9 functions filed as Phase 11 todos (`p11-*`).
- **B (intrinsic)** — pure N-arm dispatchers where CC equals the
  arm count by construction. 3 functions: `lower_one`, `lower_value`,
  `parse_expr`.
- **SKIP** — `emit_test_expr` (deferred until emit_janet retargets
  onto SSA, see task `emit-janet-from-ssa`); `parse_command_substitution`
  (already delegates `try_special_word` + `parse_statement`; no clean
  further extraction).

Wider observation: `sash-rt::sash_file_test` (CC 69),
`sash-convert::convert_command` (CC 36), `convert_declaration` (CC
27), `inject_bless_hints` (CC 22), `convert_expansion` (CC 22)
surfaced as the largest non-compiler hotspots. Out of scope for
this audit but worth a future pass.

## Action

Added `// cyclo-allow:` comments above the three pure-dispatcher
functions so the pre-commit complexity report stops flagging them:

| function | CC | shape |
| --- | --: | --- |
| `lower_one` (transform/statements.rs:27) | 17 | 17-arm StmtKind dispatcher; every arm calls one helper |
| `lower_value` (codegen/values.rs:289) | 17 | 17-arm SsaValue dispatcher; every arm calls one `lower_X_value` helper |
| `parse_expr` (parse/expr.rs:24) | 18 | adjacency-concat loop over a 10+ arm token dispatcher |

Each was already a textbook delegate. The reduction work to push
their per-arm bodies behind helpers was done in earlier waves
(j061 lifted body work out of `lower_one`; j094 split
`lower_expr_value` out of what `lower_value` calls). What remains is
pure dispatch, and refactoring further would mean replacing the
match with a less-readable lookup table for no gain.

## Verification

Filtered top-10 CC report after this commit (joining `lines` on
`start_line - 1` against `% cyclo-allow %`):

```
69 sash-rt/src/lib.rs            sash_file_test
49 sash/src/type_check.rs        check_stmt              ← p11
43 sash/src/validate.rs          validate_stmt_ctx       ← p11
36 sash-convert/src/convert.rs   convert_command
27 sash-convert/src/convert.rs   convert_declaration
26 sash/src/emit_janet.rs        emit_test_expr          ← deferred
23 sash/src/transform/entry.rs   ir_to_ssa_typed         ← p11
22 sash-convert/src/convert.rs   inject_bless_hints
22 sash-convert/src/convert.rs   convert_expansion
21 sash/src/transform/helpers.rs scan_expr_positionals_expr ← p11
```

The three annotated dispatchers no longer appear.

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

## Phase 11 queue (filed as `p11-*` SQL todos)

1. `p11-scan-arith-number` — split per-base lex helpers (CC 20→6)
2. `p11-parse-array-literal` — `try_parse_assoc_element` (CC 20→8)
3. `p11-handle-param-expansion` — `load_var_or_empty` + sequential→match (CC 18→10)
4. `p11-emit-string-op-expansion` — j097 per-op split (CC 20→8 + cyclo-allow)
5. `p11-ir-to-ssa-typed` — 3 named phases (CC 23→6)
6. `p11-scan-positionals-pair` — `record_positional` + `shift_amount` (CC 21+21→14+14)
7. `p11-validate-stmt-ctx` — j093 recipe + DD#31 helper (CC 43→15)
8. `p11-check-stmt` — j093 recipe + per-construct helpers (CC 49→12)

Cyclo-allow annotation alone is **never** a substitute for an A-
verdict refactor; these three were B by construction. The Phase 11
queue carries the genuine extractions.
