---
title: Status
modified: 2026-05-14
---

## Current branch: split-converter

## Spec status: 281 passing / 0 failing (spec.db)

### Session 2026-05-14 — completed

**Task cleanup (6 tasks)**
- Remove SsaInst::Export dead code (cleanup)
- Codegen panics → catch_unwind diagnostics (UX)
- DD#23 --manifest, --sysroot, squashfs sysroot (3 features)
- DD#57 docs — two-tool model (README, GUIDE, SUBSET)

**Quality gates**
- Added mk targets: fmt, lint, coverage, complexity, check
- cargo fmt: zero diffs (was 1137)
- cargo clippy: zero warnings (was 136)
- cargo llvm-cov: 78.1% line coverage (Rust tests + spec tests)
- contrib/cyclo.sh: rust-code-analysis-cli (Mozilla) for complexity

**Literate restructuring (major)**
- lower.rs (3043 lines) → lower/ module (7 files)
- transform.rs (4012 lines) → transform/ module (12 files)
- codegen.rs (6593 lines) → codegen/ module (12 files)
- InstCtx struct: 23-parameter lower_inst → 4 parameters
- FuncState struct: allocate_variables returns grouped state
- docs/REVIEW.md: security review reading guide + verification checklist
- TRUST/SECURITY annotations at critical code boundaries
- Max cyclomatic complexity: 277 → 140
- 22 functions > 200 lines (was 47 > 100 lines in 3 monolithic files)
- 65 source files total

### Test suite (no regressions)
- e2e: 124 / 0
- oracle: 59 / 0
- taint: 42 / 0
- codegen: 220 pass / 44 fail (pre-existing pipeline/cmdsub)
- spec: 281 pass / 0 fail

### Remaining work
- lower/expressions.rs `lower_expr` (496 lines) — needs return-type refactor
  to extract arms (some do early `return Expr{...}`, not just ExprKind)
- codegen/dispatch.rs `lower_inst` (895 lines) — remaining small inline arms
- Complexity ceiling ratchet toward CC < 50
