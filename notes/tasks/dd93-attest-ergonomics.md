status: in-progress
depends: DD93
gates:

# DD#93 attest ergonomics — live tracker

Bundle from the 2026-09-06 bless-ergonomics thread. See
`notes/decisions/DD93-attest-ergonomics.md`.

## Stages

- [x] **S1 rename** `bless` → `attest` (hard, no alias). Lexer keyword,
  identifiers/strings, diagnostics, converter emit, specs, Rust tests,
  current-state docs. 0 regressions (native/c/janet). Journal 247.
- [ ] **S2 shape tests** `i?`/`p?`/`g?`. Runtime `is_print`/`is_graph`;
  `-i`→`i?` rename; parser `?`-suffix recognizer; native/c/janet dispatch;
  migrate `types.sh` 77.x + add p?/g? specs. General operators.
- [ ] **S3 unary shorthand** `attest VAR OP` (scalar + attest-all).
- [ ] **S4 arith-assign form** `attest numstr (( myint = numstr ))`. Both
  clean, distinct names, scalar only, SB4030 exemption.

## Follow-ups (not this bundle)

- UTF-8-aware shape test (distinct spelling).
- Remaining POSIX-class shape tests (`a?` alnum, `d?` digit, `l?` alpha,
  `s?` space, …) — token/dispatch designed to make each a one-liner.
- Array analogue of the arith-assign form: `attest e in a (( … ))`.
- Same-name in-place retype for the arith form (`attest s (( s = s ))`).

## Implementation map (file:line pointers, post-S1 rename)

Line numbers drift as code changes — re-confirm with the indexnudge DB
(`sqlite3 .indexnudge-rust.db "SELECT name,file,start_line FROM symbols
WHERE name='…'"`). Lexer fact (repo memory): `?` is **not** a word-break
char, so `i?`/`p?`/`g?` already lex as a single `Tok::Word("i?")` — the work
is recognizing them as operators, not lexing them.

**Shared template — mirror the existing `-i`/`is_int` path everywhere:**
- Runtime validator `is_int_inner` (sash-rt/src/inner.rs:72) → delegates to
  `parse_strict_i64` (inner.rs:82); C-ABI wrapper `sash_is_int`
  (sash-rt/src/lib.rs:1079).
- Native extern wiring: `ExternFuncs.is_int` field (codegen/types.rs:68) +
  declaration (codegen/setup.rs:339-346).
- Parser op set: `is_unary_test_op` match (parse/parser.rs:1418); test-expr
  unary parse `try_parse_leading_unary` (parser.rs:1247); `TestExpr::Unary`
  (ir.rs:762).
- Dispatch arms (one per backend): native codegen/tests.rs:182 (`"-i"` arm;
  file tests via `file_test_op_code` tests.rs:213); c emit_c.rs:2272; janet
  emit_janet.rs:1642, with the `_isint`/`_fmtint` preamble at emit_janet.rs:52-71.
- Specs: types.sh 77.1-77.9 (plan=134), ~lines 633-660.

**S2 (i?/p?/g?):**
- Add `is_print_inner`/`is_graph_inner` beside `is_int_inner` (inner.rs:72) +
  C wrappers `sash_is_print`/`sash_is_graph` beside `sash_is_int` (lib.rs:1079).
  Non-empty; ASCII 0x20-0x7E (print) / 0x21-0x7E (graph).
- Rename `-i`→`i?`: `is_unary_test_op` (parser.rs:1418) + the 3 dispatch arms
  (tests.rs:182, emit_c.rs:2272, emit_janet.rs:1642) + types.sh 77.x spellings.
- **Gotcha:** `try_parse_leading_unary` gates on `w.starts_with('-')`
  (parser.rs:1252) — broaden it to accept the `?`-suffixed ops (which don't
  start with `-`).
- Wire 2 new externs ×3 backends (types.rs:68 field + setup.rs:339 decl model).

**S3 (attest VAR OP shorthand):**
- `parse_attest` (parser.rs:1456) + `parse_attest_all_lead` (parser.rs:1508):
  after the binder(s), if the next token is a bare unary-op word instead of
  `[[`, synthesize `ExprKind::Test(TestExpr::Unary{op, operand:$VAR})` (operand
  = the binder `$e` for attest-all) and push it as the trailing arg.
- Pure sugar: lowers onto the same args vector, so `taint_attest`
  (validate.rs:3434), `lower_if_attest` (control_flow.rs:40), `handle_attest_stmt`
  (stmt_handlers.rs:174) need no change.

**S4 (attest numstr (( myint = numstr ))):**
- `AttestForm` enum (ir.rs:451) — add an arith-convert variant;
  `classify_attest` (ir.rs:574).
- `parse_attest` (parser.rs:1456): after the scalar var, accept `(( assign ))`
  where only `[[`/`in` are allowed today (the non-`in` path errors at
  parser.rs ~1518).
- Validate: distinct-name check (source≠target); `taint_attest`
  (validate.rs:3434) clears BOTH; exempt the bare source from SB4030 —
  `check_arith_string_operands` (type_check.rs:387) / `walk_arith_var_reads`
  (type_check.rs:337).
- Transform: `handle_attest_stmt` (stmt_handlers.rs:174) / `lower_if_attest`
  (control_flow.rs:40) emit the is_int gate + abort + bind target.
- Codegen ×3: `emit_attest` (dispatch.rs:493) / `emit_attest_all`
  (dispatch.rs:634); abort via `sash_attest_fail`; janet abort-message model
  `emit_pe_require` (emit_janet.rs:2097).
