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

## Method (CORRECTED 2026 — per-function LLVM count is the reliable unit)

An earlier draft of this section claimed the coverage scan had "false
positives" (functions that ARE exercised showing as uncovered). **That framing
was itself wrong**, and the correction matters:

1. **Source-based LLVM coverage is optimization-independent and accurate.**
   Measured three ways — full `--release` (opt-level=3), `-Cinline-threshold=0`,
   and `CARGO_PROFILE_RELEASE_OPT_LEVEL=0` — the numbers are **byte-identical**
   (regions 3164 missed, lines 1753, functions 80, at the pre-cov.70 baseline).
   Source-based instrumentation maps regions through optimization, so inlining
   does NOT corrupt the counts.
2. **The reliable unit is the per-function `count`** from
   `cargo llvm-cov report --json` (`data[0].functions[].count == 0` ⇒ that
   function's body was never executed by the spec suite). This is authoritative;
   trust it.
3. **The earlier "false positives" were real never-runs.** `incdec_to_ssa`,
   `compound_assign_to_ssa`, `validate_arith_comma_in_condition`,
   `try_keys_intrinsic` were claimed "covered" — but the LLVM per-function count
   is 0 for each. What misled the earlier pass: a manual probe (`(( x++ ))`,
   `for k in $(keys m)`) produces correct output, "proving" the feature works —
   but the feature routes through a *different* path (statement-level `(( x++ ))`
   uses the statement mutation lowering, not `arith_to_ssa`'s value-arm; for-in
   `$(keys m)` uses the for-in splat path, not `try_native_intrinsic`). The
   value-context arm is separately unreachable/untested. **A passing probe of
   the feature does NOT prove a specific function ran** — confirm with the
   per-function count after adding the spec.
4. **`count > 0` after adding a spec** is the only proof a spec hit its target.
   Verified this session: cov.70→`validate_arith_comma_in_condition`,
   cov.71→`sed_malformed_err`, cov.72→`emit_string_op_expansion`,
   cov.75→`case_always_returns` all flipped 0→>0. cov.73/cov.74 pass but hit
   *adjacent* paths (for-in splat, `extract_compact_operator`) — their intended
   targets (`try_keys_intrinsic`, `extract_from_literal`) stayed 0.

### Two reliable signals for "dead vs untested"

- **`calls` table 0 callers** ⇒ structurally dead (remove).
- **caller(s) exist but per-function count = 0** ⇒ reachable-in-principle but
  the calling *path* is either (a) untested → **bucket A, add a spec**, or
  (b) shadowed by an earlier reject so no valid input reaches it → **bucket B**.
  Distinguish by probing: if any valid input flips the count, it's A; if every
  input that would reach it is rejected upstream, it's B.

## Current numbers (2026, post cov.70-75)

Compiler (native+c; janet/convert/expand/tests excluded):
**lines 92.28%** (1703/22070 missed), **functions 95.03%** (73/1469 missed),
regions 91.53%. The 1703 missed lines decompose as: lines inside the ~27
remaining named never-run functions + never-run closures (untaken branches)
+ untaken error/defensive arms *within* run functions (the bulk — bucket D).

## Method (superseded — see the CORRECTED method above)

*(An earlier draft here described an indexnudge-symbol-range vs LLVM-missing-line
cross-reference scan and called its mismatches "false positives." That whole
approach is retired: use the authoritative per-function `count` from
`cargo llvm-cov report --json` instead — see the CORRECTED method section above.
The functions it labelled false positives — `incdec_to_ssa`,
`compound_assign_to_ssa`, `try_keys_intrinsic`,
`validate_arith_comma_in_condition` — are genuinely never-run by the suite.)*

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

cov.65-69 (binary bitwise `&`/`|`/`^` in `$(( ))` [emit_arith_op], runtime
div/mod-by-zero [emit_div_by_zero_error]). cov.70-75 (targeting never-run
functions, per-function count 0→>0 verified): cov.70 `if (( a, a>0 ))`
non-final-comma-non-mutation → `validate_arith_comma_in_condition`; cov.71
`s=~ s/a/b` two-delimiter → `sed_malformed_err` (SB4032); cov.72 `${s:1:3}`
substring → `emit_string_op_expansion`; cov.75 dispatch-predicate all-arms-return
→ `case_always_returns`. (cov.73 keys-for-in, cov.74 compact-dispatch pass but
hit adjacent paths — `try_keys_intrinsic`/`extract_from_literal` stayed 0.)

Session coverage: **87.69% → 92.28%** lines (compiler, native+c); functions
**95.03%** (73/1469 never-run).

## Resolved: `trap` rejected (DG 2026-08-28)

DG: "trap needs to be purged from native." Done — direct `trap` → **SB4043**
(use `signal`/`END`; the converter owns the bash→native rewrite). The
`handle_trap` front-end machinery was removed (see Bucket B table above). This
was the same shape as `command` (DD#23-shadowed, removed 2346c36).

## Bugs surfaced by the grind (filed, awaiting DG fork decision)

The point of exercising never-run code is that unexamined paths hide bugs.
Two P1 bugs found this way (both a `bug:`/panic invariant reachable from user
source — must never happen):

1. **const-reassign panic** (`notes/tasks/const-reassign-panic.md`) — **FIXED**
   (SB4044, fork C): validate `check_const_reassignment` (spanned) + `define()`
   backstop (spanless). specs types.sh 35.3-35.9.
2. **intrinsic-arity fallthrough** (`notes/tasks/bug-intrinsic-arity-fallthrough.md`)
   — **FIXED** (SB4045, fork C): validate `check_cmdsub_arity` (spanned) +
   transform `resolve_single_cmdsubst` backstop (no bare exec, survives
   `--bypass`). specs errors.sh 55.42-45.

## Never-run named functions — classification (73 total incl. closures; 27 named)

Per-function count from `cargo llvm-cov report --json` (opt-independent,
authoritative). Every named never-run function has ≥1 caller (none structurally
dead) — so each is reachable-in-principle, classified by whether a spec can
reach it:

**A — reachable + spec-hittable (drain with specs, next rounds):**
`has_mutation` (`(( (a=1)+2 ))` nested-mutation via check_arith_nested_mutation),
`parameter_expansion_type`, `sig_scan_case`/`sig_record_arith_index` (int-func
arg-signature scan over a dispatch / arith-subscript body),
`lower_redirected_function` (`f(){…} >file`), `emit_dup_output`/`latest_var_as_str`/
`emit_arr_keys_joined` (c-backend: dup redirect, int-in-string test op, `keys`
join), `collection_reader_taint`/`bare_name_tainted` (taint of a collection
reader), `redirect_expr_zero`/`ir_expr_to_ssa_value` (redirect-target lowering),
`step_open_paren`/`scan_base_hash_span` (arith scanner sub-paths),
`skip_balanced_squote`/`skip_balanced_dquote` (dq-cmdsubst quote skipping),
`extract_from_literal`/`extract_leading_operator` (fully-glued dispatch operator).

**B — shadowed-dead candidates (probe: is every reaching input rejected?):**
`incdec_to_ssa`/`compound_assign_to_ssa` (value-context increment/compound-assign
is validation-rejected; statement form uses a different path — the `arith_to_ssa`
value-arms appear unreachable), `try_keys_intrinsic` (value-context `$(keys m)`
→ SB5025; for-in uses the splat path — `try_native_intrinsic`'s Keys arm appears
unreachable), and `validate_arith_comma_in_condition`'s *final-comma-mutation*
arm (lines 830-839, shadowed by the earlier mutation-in-condition check).
Confirm each with the remove-then-suite-run method before deleting.

**C — reachable only via non-`-c` CLI (not harness-hittable):**
`set_sysroot`, `sysroot_file_exists` (need `--sysroot` + a sysroot/squashfs tree;
`sysroot_file_exists` shells out to `unsquashfs`). Could add a file-based spec
with a fixture sysroot, or leave explained.

**D — defensive (leave in place, explained):**
`panic_payload_message` (formats a caught codegen panic — only on ICE),
`<TypeError as Display>::fmt` (error Display impl; the type-error path prints via
a different formatter), the `_ => panic!` ICE arm in `emit_string_op_expansion`.
