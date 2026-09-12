title: Per-rule integration tests for validate.rs
status: ready
priority: P7
kind: work
source: opus-47-review
depends: []
gates:
  - one test file per SBxxxx code in sash/tests/validate/
  - each test exercises both accept (valid input) and reject (rejected input)
  - mk test passes
  - coverage of validate.rs ≥ 95% lines (currently indirectly tested)

`validate.rs` is 1087 lines of safety-critical code. It absorbed
~1600 lines of CST-based rules plus 975 lines of taint analysis. The
postmortem (addendum #4) already made this point:

> If the tests are sufficient to validate rule behavior, the rewrite
> is safe — tests catch regressions. If the tests are insufficient,
> the two-parser state is ALSO unsafe.

Today the rules are exercised indirectly via `spec/*.sh`,
`brush_compat_tests`, and a handful of `taint_spec_tests`. There is no
per-rule test file.

## Scope

For each SB code currently emitted by `validate.rs`, create
`sash/tests/validate/sb{NNNN}.rs` (or a single file with one `#[test]`
per code). Each test:

1. Compiles a snippet expected to **pass** validation cleanly.
2. Compiles a snippet expected to **fail** with the specific SB code.
3. Optionally: a snippet expected to fail with a *different* SB code,
   confirming the rule scope.

## SB codes that live in validate.rs today

(from a grep of `validate.rs`):
SB5001, SB5002 (heredoc edge), SB4001, SB5006, SB5007, SB5009, SB5011,
SB5012/5013/5014, SB5017, SB5018, SB5019, SB5020 (taint), SB5023.
Plus the SB4xxx codes the parser emits but that validate.rs may also
gate.

## Why P7 (medium effort, no decision needed)

This unblocks aggressive refactoring of `validate.rs` itself (current
max CC inside it is `validate_stmt_ctx` at 43, fine, but the next
restructure will be safer with per-rule tests).

Also lets the next adversarial audit aim at specific rules with
known coverage.

## Out of scope

- Re-implementing any rule.
- Re-architecting the file. Tests first; structure later.

## Re-measured 2026-09-12 — the filing's sizing is off by 6.6x

`validate.rs` is **7222 lines**, not the 1087 the filing states. The scope
described ("one test file per SB code") is therefore far larger than the
estimate it was filed under. `sash/tests/validate/` still does not exist.

Re-derive the SB-code inventory and re-estimate before scheduling; the listed
codes (SB5001, SB4001, SB5006, …) were enumerated against a file less than a
sixth of the current size.
