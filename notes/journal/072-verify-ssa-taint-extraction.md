# 072 — verify_ssa_taint two-phase extraction

## Goal

Reduce `ssa_taint.rs::verify_ssa_taint` (CC 36) to a phase-orchestrator
shell, per the audit's recommendation to extract `classify_inst_def`
and `check_inst_uses`.

## Approach

The function had four logical phases:

1. Seed clean entries for specials and positional params.
2. Collect user-function names from `fn_*` blocks.
3. Phase 1: classify the VarRef defined by every instruction (nested
   inst dispatcher inside the per-block loop, with phi handling at the
   top of each block).
4. Phase 2: report uses of tainted values that escape into output,
   externals, or index expressions.

Lift each to a named helper:

- `seed_clean_specials(taint)` — 3 inserts blocks of magic-var rules
- `collect_user_functions(program)` — returns `HashSet<String>`
- `classify_block_defs(block, taint, user_functions)` — handles phi
  then delegates per-inst to `classify_inst_def`
- `classify_inst_def(inst, taint, user_functions)` — the inst dispatch
- `read_line_taint(redirects, clean_pipe)` — folds the duplicated
  herestring/heredoc + clean_pipe → Taint check between `ReadLine`
  and `ReadLineMulti`
- `check_block_uses(block, taint, errors)` — simple per-inst loop
- `check_inst_uses(inst, taint, errors)` — the inst dispatch with
  `check_value_use` calls

The orchestrator becomes 12 lines: seed, collect, two block loops.

## Result

| function | CC |
| --- | --: |
| `verify_ssa_taint` | 36 → **3** |
| `check_inst_uses` | 10 |
| `classify_inst_def` | 8 |
| `classify_block_defs` | 5 |
| `read_line_taint` | 5 |
| `seed_clean_specials` | 3 |
| `collect_user_functions` | 3 |
| `check_block_uses` | 2 |

Max helper CC 10. No new whitelist needed.

## Verification

- `cargo build --release` — clean
- `cargo clippy --release --lib -- -D warnings` — clean
- `cargo test --lib --release` — 12/12
- `spec/run.sh` — 291/291

## Notes

- One typo on the first try: `&SsaBlock` (the type is `BasicBlock` —
  there's no `SsaBlock` alias in `ssa.rs`). Caught by `cargo build`.
- The phi inner-loop comment about iterative correction (loops won't be
  caught) is preserved verbatim in `classify_block_defs`; behaviour is
  bit-for-bit identical.
- `read_line_taint` is the smallest meaningful share between
  `SsaInst::ReadLine` and `SsaInst::ReadLineMulti` — the herestring-or-
  heredoc-or-clean-pipe condition was literally duplicated.
- This is the airbag/backstop pass: it doesn't enforce anything new,
  it just verifies the CST analyzer didn't miss. The refactor preserves
  every classification rule.
