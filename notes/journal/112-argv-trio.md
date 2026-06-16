---
title: argv / arg / argc trio — DD#60, top-level positional rejection, inside-fn $@ fix
modified: 2026-06-16
---

# Session 2026-06-16 — argv/arg/argc trio (DD#60)

Implemented the full argv-trio task chain (argv-trio-*), closing
`bug-for-in-dollar-at-taint-ice` (top-level `for x in "$@"` ICE) and
landing the DD#60 program-argument surface end-to-end on the native and
c backends.

## Surface (DD#60, supersedes the argv half of DD#42)

- `$(argc)` — C argc (1 + user-arg count); **clean** (a count, like `$#`).
- `$(arg N)` — scalar program arg, 0-based (`arg 0` = program name);
  **tainted**; aborts at runtime on N ≥ argc.
- `$(argv)` / `$(argv N)` — argv **array splat** (slice from N, Python-style
  `argv[N:]`); **tainted**; legal only as a for-in sole source or an
  `array a=$(argv N)` snapshot RHS. Out-of-range N → empty (no abort).
- `array a=$(argv N)` — tainted snapshot (DD#61 bulk-read source); bless via
  bless-all (`bless x in a [[…]]`).

## What landed

**Validator (`validate.rs`).** Four new passes/codes:
- SB5024 — `check_positionals`: top-level `$@/$*/$1..$N/$#` rejected with a
  directed message (`$0` stays SB5017, message updated to `$(arg 0)`).
  Inside a function body they're accepted (clean). This is what makes the
  old ICE path unreachable.
- SB5025 — `check_argv_splat`: `$(argv)`/`$(argv N)` in any non-splat
  context (scalar assign, command arg, test, arithmetic).
- SB5026 — quoted array splat (`"${a[@]}"`, `"$a[@]"`, `"$(argv)"`) per the
  DD#22 addendum (quotes always yield one string).
- SB5027 — for-in header mixing a tainted argv splat with any other source.
- Plus: a **bare tainted command-substitution at a use site** is now
  rejected (`check_expr_taint` gained a `CommandSubstitution` arm). This
  closed a pre-existing taint escape — `echo $(getenv HOME)` printed
  `$HOME` with no bless. `$(argv)` is excluded here (owned by SB5025).
- `arg` added to BUILTINS + the taint-source sets (`cmdsub_is_taint_source`,
  `ssa_taint.rs`). The implicit top-level `$1-$9` taint seed was removed
  (superseded by SB5024).

**Codegen (native).** `$(argc)` → C argc (dropped the `-1`); `$(arg N)`
intrinsic via `sash_argv_get` (runtime made 0-based, `arg 0` now legal).
`for x in $(argv N)` repurposes the old `$@` for-loop into
`lower_for_argv_slice(start)` — `__fi = start`, condition `__fi < $(argc)`
materialised into `__argc`, loop var `ArgvIndex` (now **0-based direct**,
argv[idx]). `array a=$(argv N)` → new `SsaValue::ArgvSnapshot { start }`
→ runtime `sash_argv_snapshot(start)` building an indexed table.

**Inside-function `$@` (second bug found).** `$@`/`$*` inside a function
were splicing the *program* argv, not the caller's args (cf.f11/f12 gave
wrong output). Fixed by expanding `$@/"$@"/$*` to the explicit positional
list `$1..$N` at inline time (`expand_dollar_at_in_body`), with N = the
call's argc. The standalone function-template lowering expands `$@` to
empty (no caller) so the dead template never emits a tainted ArgvIndex.

**Length-on-tainted exemption.** `len a` / `${#a[@]}` / `${#s}` (`arrlen` /
`strlen`) are clean even on a tainted array — a count exposes no element
data (DD#60 "counts need no bless"). Exempted in `ssa_taint.rs`.

**c oracle (`emit_c.rs`).** Added argc/arg/ArgvIndex/ArgvSnapshot — the
runtime `_start` already sets the argv globals and calls `main`, so the
linked `sash_argv_get`/`sash_argv_snapshot` just work. `argv.sh` passes
under `--emit-c`.

**sash-convert.** Context-aware (DD#46) top-level rewrite: `$N → $(arg N)`,
`$@/$* → $(argv)`, `$# → $(($(argc) - 1))`, `$0 → $(arg 0)`; inside-function
forms preserved (`has_ancestor_kind(node, "function_definition")`). Splats
inside `"$@"` drop the quotes (`splat_replace_node`). The old never-shipped
`$(argv @)` target is gone.

## Spec

New `spec/argv.sh` (26: av.t/s/n/x/m). Extended control_flow (cf.dd46.1-6,
cf.f11-14; cf.f4/f7 migrated), wordsplit (ws.q1-5; 28.2 → `@a`), errors
(e.dd46.t1-5), convert (conv.f1-3, conv.q1; 57.13a/b retargeted). Migrated
functions.sh 42.x and taint.sh 24.2/51.1 off the old `$(argv N)` scalar.
Fixed two **pre-existing** plan-count off-by-ones (errors.sh, the new
convert additions).

**Result (spec.db, deduped test_status):** native 402 passing / 2 failing
(only the pre-existing `cf.c7`/`cf.c8` case-glob bugs). Zero regressions in
any mode — c improved (66→62 failing; argv parity added), janet +19 are
all new argv tests on the unsupported backend (no janet test regressed).

## Bugs found (DG rule: fix spec-failing, file the rest)

1. for-in `$@` ICE — **fixed** (the task).
2. inside-fn `$@` splicing program argv — **fixed** (cf.f11/f12).
3. bare tainted cmdsub at use site not rejected (getenv leak) — **fixed**.
4. `cf.c7`/`cf.c8` case-glob — pre-existing, unrelated, tracked by
   `bug-case-pattern-glob-not-validated`. Left alone (no regression).
5. **Rust suite rot** — `mk test` is ~158-red at HEAD (declare-syntax,
   SB4020→SB5020, output mismatches), independent of this work. Filed
   `rust-test-suite-rot`. The argv-trio added zero net rust regressions
   (migrated the 8 flipped argv/positional/quoted-splat tests).

## Filed for follow-up

- `emit-janet-argv-parity` (P2) — janet backend argv (Janet-native args).
- `rust-test-suite-rot` (P1) — the pre-existing 158.
- `converter-command-arg-splat` (P2) — `cmd "$@"` has no clean target;
  should be a converter MANUAL marker, not non-compiling `cmd $(argv)`.

## SB codes added: 5024, 5025, 5026, 5027.
