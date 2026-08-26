# 188 — spec-suite code coverage baseline + dead-code removal

DG asked to (a) understand the code coverage the spec suite provides, (b)
delete the dead code it surfaced, and (c) push spec coverage toward every
line that a spec *can* exercise (ignoring janet).

## Coverage baseline (this session)

Method: instrumented `sash` (LLVM source-based coverage, `cargo llvm-cov
show-env`), ran the full spec suite so each `sash` invocation drops a
profraw, aggregated. Scoped to the compiler (`sash/src`, excluding
sash-rt/convert/expand, tests, main.rs).

- All 3 modes: **85.1% lines / 87.6% fns / 83.9% regions** (24692 lines).
- native+c only (ignoring janet): **87.5% lines / 91.5% fns** (22373 lines,
  2789 missed).

Best-covered: validate 91%, ssa_taint 92%, codegen/arithmetic 98%,
inst_handlers 95%, emit_c 95%, statements 93%, resolve 94%. Gaps concentrate
in the janet backends (emit_janet_map 0% — `--janet-map` isn't in the suite;
emit_janet 82%) and codegen corners (dispatch 78%, redirects 68%).

**Caveat — release-mode + dispatch shape.** Coverage was measured on the
release build. Some "uncovered" lines are NOT spec-reachable: `to_tok`'s
keyword arms (the lexer dispatches keywords through `scan_word`, only
structural bytes reach the logos path, so the keyword→Tok arms are
compiler-unreachable — a unit test covers them); and defensive/`unreachable`
arms.

## Dead code removed

Coverage surfaced genuinely dead code (all `pub`, so `dead_code` lint was
blind to them; 0 call sites in `src/` or `sash/tests/`):

- `ssa.rs`: `BasicBlock::predecessors()`, `ArithOp::as_u8()`,
  `ArithOp::from_str()` (ops are built via `from_arith`).
- `diagnostic.rs`: `Diagnostic::deferred_at()` (the lone `Severity::Deferred`
  constructor; the Deferred mechanism itself — SB4004 var-as-command,
  referenced in DDs — is left in place as potentially-deliberate, just its
  dead constructor removed).

Verified: `cargo clippy --tests -D warnings` clean, full native/c/janet
sweep 0 regressions, native gate 0.

## Repeatable workflow

`cargo llvm-cov show-env --sh` → `cargo llvm-cov clean --workspace` → build
instrumented `sash` → run specs with the default `SASH_RELEASE_DIR`
(target/release, now instrumented) → `cargo llvm-cov report --release
--ignore-filename-regex '(sash-rt|sash-convert|sash-expand|/tests/|tests\.rs|
main\.rs|emit_janet|oracle\.rs)' --show-missing-lines`. NB: the clean wipes
`sash-convert`/`sash-expand`; rebuild the whole workspace (`cargo build
--release`) before running the converter/expand specs or they spuriously
fail.

Coverage-driven spec additions follow in later journals.
