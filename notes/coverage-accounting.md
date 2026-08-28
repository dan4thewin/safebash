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

## Bucket B — shadowed-dead, REMOVAL PENDING A DG DECISION

- **`exec` builtin cluster** — `emit_exec` / `emit_exec_static_argv` /
  `build_argv_on_stack` / `emit_exec_redirects_only` / `finalize_exec`
  (~90 lines, `codegen/dispatch.rs`). `"exec" => BuiltinCmd::Exec` is in
  `from_name` and the codegen exists, but **resolution rejects `exec` as an
  unresolved command (SB5011)** before the `BuiltinCmd::Exec` dispatch is ever
  reached — exactly the `command` pattern, but with **no explicit DD** saying
  `exec` is out of sash. Needs DG: is the `exec` builtin (process replacement /
  fd redirect) intended to be in sash (→ fix resolution to recognize it) or
  not (→ remove the dead codegen)? Until decided, this is reachable-in-principle
  code that no spec can hit.

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
`[[ ( … ) ]]` grouped test [parse_test_group]) + `trace.sh` (DD#32). See
journals 190-199.

## Bucket B — accepted-in-source bash forms that the DESIGN makes converter-input (DG decision pending)

These are bash spellings that sash currently **accepts and lowers** in source,
but the design says the native form is different and the bash form is the
converter's input. So the accept-machinery is reachable only via a form that
shouldn't be written by hand — a candidate to **reject** (directing to the
native form), which turns the machinery into dead/reject code (cf. `command`,
removed in 2346c36).

- **`trap`** — DD#56 makes the native signal model `signal N {IGN,DFL,FLAG,EXIT}`
  + `END { }`; `trap` is bash the converter rewrites to those (signals.sh 57.14
  is a `run_convert` test). Yet `"trap" => BuiltinCmd::Trap` + `handle_trap` /
  `handle_trap_ignore_reset` / `signal_name_to_num` accept & lower direct `trap`
  in sash source — and buggily (a signal command handler aborts with `bare
  command passed to sash_exec`). The converter emits `signal`, never `trap`, so
  this machinery is reachable ONLY via hand-written `trap`, which DD#56 says
  isn't a native form. **Removed the mistaken cov.60/65/66 specs that pinned it.**
- **`exec`** — `"exec" => BuiltinCmd::Exec` + `emit_exec`/`emit_exec_static_argv`/
  `build_argv_on_stack`/`emit_exec_redirects_only`/`finalize_exec` exist, but
  resolution rejects `exec` as an unresolved command (SB5011) before dispatch.
  No explicit DD.

**DG decision needed:** should direct `trap` and `exec` in sash source be
**rejected** (SB error → use `signal`/`END`; use the resolved path), making
these clusters dead-and-removable? Or kept as bash-compat (then the trap
signal-handler bug must be fixed and `exec` resolution wired)?

## TODO — candidates still to classify (verify empirically, then file here)

`scan_base_hash_span` (arith `base#digits`), `skip_balanced_dquote`/
`skip_balanced_squote`, `extract_from_literal` (compact `[[ ]]` — dispatch specs
may already cover; the scan is a suspected false positive), `lower_redirected_function`
(`fn f{}; f > file` — works, likely a real gap), `sysroot_file_exists` (needs
`--sysroot` + a sysroot tree), `sed_malformed_err`, `extract_leading_operator`,
`build_argv_on_stack`/`emit_exec_*` (part of the exec cluster — see the pending
DG decision above). `panic_payload_message` → likely bucket D (panic recovery).
