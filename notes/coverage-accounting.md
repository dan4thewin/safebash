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
| `ArithOp::display` (dead operator→string helper, NOT an SSA pretty-printer) | 0 callers — codegen matches `ArithOp` directly | (this session) |

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

*(none currently — the one prior entry was a mischaracterization; see below.)*

## Correction — there is NO SSA pretty-printer (DG 2026-08-28)

An earlier draft of this file listed "`ssa.rs::Display` — the SSA
pretty-printer" as a Bucket-C item. **That was wrong on both counts** (DG caught
it):
1. **There is no SSA pretty-printer.** No SSA type (`SsaProgram`/`BasicBlock`/
   `SsaInst`/`SsaValue`) has a `Display`/`fmt` impl, and no CLI flag dumps SSA.
2. The symbol the scan flagged (`ssa.rs:435`) was **`ArithOp::display`** — a
   trivial 19-arm `match` mapping each arithmetic operator to its symbol string
   (`Add => "+"`). The cc=20 was just the wide match; it's not complex, and
   it's not a pretty-printer.

`ArithOp::display` turned out to be **dead code** — zero callers (native codegen
matches `ArithOp` directly in `emit_arith_op`; emit_c hardcodes the operator
strings; a `pub fn`, so no dead-code warning fired). Removed and verified
(compiles clean, full native suite 0 regressions). Moved to Bucket B below.

This is another instance of the scan-false-positive caveat: the "cc=20" +
"uncovered" signals looked like a complex unhittable function but were a dead
lookup table. Always resolve the actual symbol + callers, don't trust the
scan's label.

## Bucket A — specs added this session

cov.29-64 (arith operators, ternary + laziness, `command`/slurp rejects,
for-in-cmdsubst, backtick-in-quotes, ANSI-C `$'\ooo'` octal [scan_ansi_c_octal],
`[[ ( … ) ]]` grouped test [parse_test_group]) + `trace.sh` (DD#32) + signals.sh
56.14/56.15 (`trap`→SB4043) + `exec.sh` ex.1-11 (the exec cluster: emit_exec /
emit_exec_static_argv / build_argv_on_stack / finalize_exec /
emit_exec_redirects_only — now 0 missing lines). See journals 190-201.

Session coverage: **87.69% → 91.91%** lines (compiler, native+c).

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
