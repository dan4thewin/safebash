# 060 — extracting handle_pipeline transform helpers

## Background

`handle_pipeline` in `sash/src/transform/stmt_handlers.rs` was
the #2 hotspot after the param-expansion refactor (CC 54, 213
lines). It dispatched on the *shape* of the pipeline's last
stage — pipe-while-read, mapfile, lastpipe-read, or regular —
with three deeply-nested branches sharing a near-identical
producer-stage loop.

Task `p3-handle-pipeline-transform`, audit Wave 2 #3.

## What changed

Followed the audit recommendation verbatim: `build_producer_stages`,
`parse_read_flags`, sub-handlers per pipeline shape. Plus two
small classification predicates that were repeated inline.

Seven new helpers:

1. `is_mapfile_last(last) -> bool` — classification predicate.
2. `is_lastpipe_read_last(last) -> bool` — classification predicate.
3. `build_producer_stages(cmds, state) -> Vec<(name, args, redirects)>`
   — shared by pipe-while-read and lastpipe-read shapes (was
   duplicated 16 lines × 2).
4. `parse_read_flags(args) -> (var_names, delim, field_sep)` —
   parses `read -d <c> -f <c> vars...` argument list.
5. `handle_pipe_while_or_mapfile(p, last, is_mapfile, ssa, state,
   block) -> BlockId` — wraps the pipe-into-while-read /
   pipe-into-mapfile path; emits Pipeline inst then patches fds.
6. `handle_lastpipe_read(p, last, ssa, state, block)` — DD#20
   lastpipe path; emits Pipeline + ReadLine/ReadLineMulti.
7. `handle_regular_pipeline(p, ssa, state, block)` — regular
   `cmd1 | cmd2 | ...` including compound-stage forking.

Parent is now a clean dispatcher: classify last stage → branch
to one of three handlers → handle single-stage case → apply
negation.

While extracting, replaced two `starts_with("-d") { get(2) }`
patterns with `strip_prefix` (clippy-cleaner). No behavior
change otherwise.

## Results

| function                         | before | after |
| -------------------------------- | -----: | ----: |
| `handle_pipeline`                |     54 |     7 |
| `parse_read_flags`               |      — |     9 |
| `handle_pipe_while_or_mapfile`   |      — |     8 |
| `is_mapfile_last`                |      — |     7 |
| `build_producer_stages`          |      — |     7 |
| `handle_regular_pipeline`        |      — |     7 |
| `is_lastpipe_read_last`          |      — |     5 |
| `handle_lastpipe_read`           |      — |     5 |

Parent CC 54 → **7**, far better than the audit's projected
range. Max helper CC is 9. `handle_pipeline` drops out of the
top-20 entirely.

Verification:
- `cargo build --release`: clean
- `cargo clippy --release -D warnings`: clean
- `cargo test -p sash --lib`: 12/12
- spec: 291/291

(Pre-existing: `brush_compat_tests` and `sash-rt-tests` fail
with linker errors about undefined `sash_streq`/`sash_arith_eval`
symbols in the runtime test harness. Reproduced on `git stash`
of my changes — unrelated to this refactor.)

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 55 | `lower_one` | `sash/src/transform/statements.rs:27` |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` |
| 48 | `lower_value` | `sash/src/codegen/values.rs:261` |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` |
| 47 | `scan_word` | `sash/src/parse/lexer.rs:1095` |
| 48 | `handle_command` (transform) | `sash/src/transform/stmt_handlers.rs:18` |

Next per audit Wave 2 order: `handle_command` transform (48),
then `lower_value` codegen (48). `lower_one` (55) is C→A
(audit verdict): straight StmtKind dispatcher, easy extraction.
`print_arith_expr`, `to_tok`, `arith_expr_to_janet` are
WHITELIST (B-verdict, op-table dispatchers — splitting would
hurt readability).
