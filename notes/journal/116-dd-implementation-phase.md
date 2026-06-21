---
title: 116 — DD implementation phase (2026-06-21)
---

# DD implementation phase

After the 2026-06-21 decision review (all DD#57, DD#62–69 resolved;
three concrete decisions made), DG directed a new implementation sweep:
same SOP as the last backlog round — work ready tasks in dependency
order, file decisions/non-blocking bugs as tasks, fix blocking bugs
inline, brief running commentary here, don't stop for status, switch
when done/blocked, don't pre-defer on predicted effort. Strive to make
progress on every ready task (and those that become ready).

Dependency-ordered plan: cstring-in-double-quotes → array-splat
(unblocks glob-splat-disposition) → reject-undeclared-scalar-read →
vet-var-modifiers → dd67-sugar-sweep → regex-builtins → dd66-dispatch
(unblocks dd68-brace-cf) → backend/infra tasks.

## cstring-in-double-quotes (DD#64 addendum) — starting

`$'…'` recognized inside `"…"` as an escape-aware compile-time constant,
so every `$`-form expands in `"…"`. Parser-only: `scan_double_quoted`
hands off `$'` to the existing escape-aware `scan_ansi_c_quoted`.

## cstring-in-double-quotes — DONE

Implemented in `scan_dq_dollar` (lexer.rs): the `b'\''` arm after `$`
skips the apostrophe and hands off to `scan_ansi_c_quoted`, appending
the resolved bytes to the literal buffer (C-string is compile-time
literal text, not a separate part). Built clean; clippy clean.

Spec tests 64.4–64.8 (`spec/wordsplit.sh`): `"a$'\t'b"` → `a<tab>b`;
literal `"` via `$'\x22'`; escape-aware close; lone apostrophe stays
literal; `$'` claims its sequence. All pass in native, c, AND janet
(15/15 in spec.db). Plan bumped 44→49.

Verification: native spec holds at 2 failures (pre-existing cf.c7/cf.c8
case-glob). Rust suite delta vs the after-dd64 baseline (+5 names) is
pre-existing churn — the same 5 (file_slurp, function_called_twice,
function_exit_status_true, negation_with_and, return_with_status) fail
identically with my lexer change stashed, so not a regression (oracle
order-dependence).

Side find (filed, not chased): `spec/wordsplit.sh` emits one more TAP
result in native than c/janet — pre-existing (committed plan=44, native
emitted 45). Task: spec-wordsplit-count-desync.
