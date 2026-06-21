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

## array-splat-command-arg (A1+B1+C+D) — DONE

`@a` (= `${a[@]}`) in command-arg position now expands to one verbatim
argv entry per element (DD#20/DD#22), in all three backends.

- **A1**: new `SsaValue::ArraySplat { array }`. Produced only at command-
  arg sites via a new `arg_to_value` helper (expressions.rs) routed
  through every arg-builder (cmd_handlers, stmt_handlers, control_flow,
  expr_handlers CmdSubst). Taint inherits from the named array
  (ssa_taint.rs, like TableValue). lower_value ICEs on a scalar
  ArraySplat — it must be expanded by the argv builder.
- **B1 native**: `build_argv_values` (codegen/commands.rs) unifies all
  three native argv builders (lower_exec, lower_exec_stage, cmdsubst).
  No splat → fixed stack slot (byte-identical to before). Splat →
  runtime argc (`#scalars + Σ sash_table_len`), `sash_alloc`, Cranelift
  fill loops (`sash_table_value` per element; ptrs are NUL-terminated by
  the ptr[len]==0 invariant). Empty array → zero entries.
- **C**: `emit_argv` / `emit_argv_splat` (emit_c.rs) — runtime C99 VLA.
- **D**: `janet_command_parts` (emit_janet.rs) — Janet `;` splice into
  both os/execute and os/spawn tuples.

Specs wordsplit 22.14-22.19 (basic, no-split-within-element, mixed,
multi-splat, empty→zero-args, pipeline). 22.14-22.17 pass native/c/janet;
22.18-22.19 (pipelines) native/c only — Janet os/spawn pipes are
pre-existing flaky (filed janet-pipeline-eagain-flaky).

**Spec hygiene fix (folded in):** `wordsplit.sh` had a misplaced `tally`
mid-file; appended "Additional coverage" tests ran only in native. Moved
`tally` to EOF, plan 49→63. Closes spec-wordsplit-count-desync. Surfaced
two pre-existing c/janet gaps (39.10 regex \w, 22.12 echo|read) → filed
cjanet-wordsplit-39-10-22-12.

Native spec baseline preserved (2 failing = cf.c7/cf.c8). Rust suite
184=184 (only the known oracle-order flakiness differs). Found a separate
pre-existing bug: `$(printf …)` passes a bare command to sash_exec
(builtin not abs-path-resolved in cmdsubst) → filed
bug-cmdsubst-builtin-not-resolved (cmdsubst splat verified working with
an explicit abs path).
