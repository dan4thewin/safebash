# 078 — handle_command (transform) extraction

## Goal

Reduce `transform/stmt_handlers.rs::handle_command` to a one-glance
orchestrator. Audit recipe: per-shape `try_X` helpers (pattern #3).
Phase 8 BuiltinCmd refactor already brought it 48 → 26; this finishes
the job.

## Approach

Four lifts, plus a structural merge:

1. `try_builtin(kind, …) -> Option<BlockId>` — collapses both
   pre-regex and post-regex BuiltinCmd matches into a single 11-arm
   dispatcher. Most arms return `Some(handler(…))`; `Test` and
   `Mapfile` return `Option<BlockId>` directly so they fall through
   to the regex/general path on no-match. Tagged `// cyclo-allow:` —
   arms == CC by construction.
2. `try_test_arg_capture(c, ssa, block) -> Option<BlockId>` — the
   `Test`-arm body. Scans args for `ExprKind::Test` and emits
   `SsaInst::Test`. None when no Test arg present.
3. `try_regex_substitution(c, name, state, ssa, block) -> Option<BlockId>` —
   the `var s/pattern/replacement/flags` block. Single-arg form;
   uses `?` on `strip_prefix("s/")` for early exit. Preserves DD#39
   compile-time regex validation and the SB5023 diagnostic.
4. `check_non_void_bare_call(name, stmt, state)` — DD#55/SB4016
   non-void-bare-call diagnostic. Side-effecting helper (no return).

The combined-dispatch is the structural win: ordering is preserved
because `Test`/`Mapfile` returning `None` flows through to the regex
check and then the general handler, exactly matching the original
two-stage match.

## Result

| function | CC |
| --- | --: |
| `handle_command` | 26 → **3** |
| `try_builtin` | 13 (11-arm + `_`; `cyclo-allow`'d) |
| `try_regex_substitution` | 8 |
| `try_test_arg_capture` | 3 |
| `check_non_void_bare_call` | 3 |

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. The pre/post-regex split was structurally
  motivated (Test arm may not consume); merging into one
  Option-returning dispatcher achieves the same semantics in one
  pass.
- `handle_return_stmt` is the only sub-handler that needs
  `&mut block`; threaded through `try_builtin`'s signature.
- The audit row's "shift" mention is a typo for shape — `Shift` is
  already an unambiguous BuiltinCmd arm; the real shapes were Bless,
  Test, regex, and the various always-consume builtins.
- SB5023, SB4016 wording preserved verbatim.
