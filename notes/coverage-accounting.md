# Coverage accounting — every reachable block explained

DG's goal (2026-08-28): *"ultimately we should have an explanation for every
block of code that is reachable but cannot be hit with a spec."* This file is
that living account. A line of compiler code (native+c backends; janet
excluded) is in exactly one bucket:

- **A. spec-hittable, now tested** — a spec exercises it.
- **B. shadowed-dead → removed** — an earlier pass rejects every input that
  would reach it; deleted (verified: full suite stays green, per DG's
  remove-then-run method).
- **C. reachable but NOT spec-hittable** — needs an explanation here.
- **D. defensive / unreachable** — `panic!`/`unreachable!`/impossible arms;
  left in place, explained.

## Method (important: the "fully-uncovered function" scan is unreliable)

The scan that cross-references the LLVM missing-line map with the indexnudge
`symbols` line ranges has **false positives**: functions that ARE exercised by
specs can show as uncovered when coverage attributes their lines to a different
region than the index's symbol range. Confirmed false positives this session:
`incdec_to_ssa`/`compound_assign_to_ssa` (C-style `for (( i++ ))` is tested —
control_flow.sh 141/145), `try_keys_intrinsic` (`$(keys m)` — types.sh 75.7),
`validate_arith_comma_in_condition` (cov.1-3/48). So **every candidate must be
verified empirically** (feed an input, observe behavior) or via the `calls`
table (0 callers = truly dead) — not by the scan alone.

## Bucket B — shadowed-dead, removed this session

| code | shadowed by | commit |
|------|-------------|--------|
| `$@`-expansion transform + `$@`-exec codegen | DD#81 `$@`→SB5045 | 29bdd25, ed95432 |
| `emit_command_builtin` + `CommandBuiltin` chain | DD#23 `command`→SB4001 | 2346c36 |
| `init_numeric_positional` + `init_at_var` | top-level positionals→SB5024/5045; DD#60 args-array | 211c464 |
| `emit_strlen_elem` (element-length) | never had a producer | ddeb1d4 |
| `handle_trap` + `handle_trap_exit`/`_ignore_reset` + `Trap` variant | DD#56: `trap`→SB4043; converter owns bash→signal/END | (this session) |

## `exec` — SUPPORTED (DG 2026-08-28), native

DG: `exec` has no replacement for its two use cases and must be supported. Done
(journal 201, DD#14 amended):
1. **fd manipulation** — `exec > file` etc. applies redirects to the running
   process (persist, no fork).
2. **program replacement** — `exec CMD args` execve's the process (no-return).

The target follows DD#23/#24 like any command: literal external path
(variable→SB5011, tainted arg→SB5020, bare name→resolved external path,
recorded in `--manifest`). specs `spec/exec.sh` ex.1-11 (native). The exec
codegen cluster (emit_exec/emit_exec_static_argv/build_argv_on_stack/
finalize_exec/emit_exec_redirects_only) is now covered. **Native-only** — c/janet
don't emit exec yet (task `c-janet-exec-support`).

## Bucket C — reachable but not spec-hittable (explained)

- **`ssa.rs::Display` (`display`, cc=20)** — the SSA pretty-printer. **No CLI
  flag emits SSA** (`lib.rs` has `--janet`/`--emit-c`/`--extract`/`--annotate`
  but no `--emit-ssa`/`--dump-ssa`); the Display impl is used only by Rust unit
  tests and ad-hoc debugging. Not reachable from any `sash` invocation, so no
  spec can hit it. (Options for DG later: add a `--dump-ssa` debug flag to make
  it spec-coverable, or accept it as test-only.)

## Bucket A — specs added this session

cov.29-64 (arith operators, ternary + laziness, `command`/slurp rejects,
for-in-cmdsubst, backtick-in-quotes, ANSI-C `$'\ooo'` octal [scan_ansi_c_octal],
`[[ ( … ) ]]` grouped test [parse_test_group]) + `trace.sh` (DD#32) + signals.sh
56.14/56.15 (`trap`→SB4043). See journals 190-200.

## Resolved: `trap` rejected (DG 2026-08-28)

DG: "trap needs to be purged from native." Done — direct `trap` → **SB4043**
(use `signal`/`END`; the converter owns the bash→native rewrite). The
`handle_trap` front-end machinery was removed (see Bucket B table above). This
was the same shape as `command` (DD#23-shadowed, removed 2346c36).

## TODO — candidates still to classify (verify empirically, then file here)

`scan_base_hash_span` (arith `base#digits`), `skip_balanced_dquote`/
`skip_balanced_squote`, `extract_from_literal` (compact `[[ ]]` — dispatch specs
may already cover; the scan is a suspected false positive), `lower_redirected_function`
(`fn f{}; f > file` — works, likely a real gap), `sysroot_file_exists` (needs
`--sysroot` + a sysroot tree), `sed_malformed_err`, `extract_leading_operator`,
`build_argv_on_stack`/`emit_exec_*` (part of the exec cluster — see the pending
DG decision above). `panic_payload_message` → likely bucket D (panic recovery).
