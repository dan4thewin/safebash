# Session 2026-04-25 (overnight) — Codegen Panic Cleanup + Brush Triage

**Goal:** Convert user-triggerable panics to proper implementations or
diagnostics. Re-run Brush test suite and triage.

### Codegen Improvements

**New test operators implemented:**
- `[[ f1 -ot f2 ]]` — file older-than, via reversed args to `sash_file_newer`
- `[[ f1 -ef f2 ]]` — same device+inode, new `sash_file_same_dev` in sash-rt
- `[[ -v var ]]` — variable set test (treated as -n; will need refinement
  for "set to empty" vs "unset" distinction)

**Panic cleanup:**
- All 32 `panic!()` calls in codegen.rs prefixed with `ICE:` for
  recognizability as internal compiler errors
- `eprintln!` for unknown unary test operator → ICE panic (was silently
  producing wrong code)
- `eprintln!` for unsupported param expansion → ICE panic
- Undefined variable in param expansion → returns empty string (bash behavior)
  instead of panicking
- Known env vars (from `known_externals.rs`) pre-seeded as clean in SSA
  taint verifier, fixing false ICEs on `$HOME`, `$USER`, etc.

3 new codegen tests: `test_file_older_than`, `test_file_same_device`,
`test_file_ef_different`. 505 tests total, all green.

### Brush Suite Triage

Re-ran 622 cases. Results vs baseline:

| | 2026-04-22 | 2026-04-25 |
|---|---|---|
| PASS | 173 | 160 |
| FAIL_OUTPUT | 78 | 59 |
| FAIL_COMPILE | 43 | 75 |
| NA | ~300 | 300 |

PASS went down because 54 scripts now correctly hit the taint gate
(SB4020). FAIL_OUTPUT improved from 78 → 59. Compile failures went
up for the right reason — the compiler is stricter.

Top output failure categories:
- Parameter transformations (`${var^^}`, `${var@Q}`, etc.) — 7 cases
- Compound command redirects — 3 cases (already on TODO)
- Arithmetic side effects (`(( x = 1 ))`) — 3 cases
- Array substring replacement — 5 cases
- Continue/break N — 2 cases

Full triage in `audit/run-2/BRUSH_TRIAGE.md`.

DG directed architecture and design. AI implemented.
