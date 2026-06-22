---
title: 117 — implementing the reviewed decisions (2026-06-22)
---

# Implementing the decided tasks

After the 2026-06-21 decision review (5 decisions resolved with DG), DG
directed implementation with the recent SOP: dependency order, file
decisions/non-blocking-bugs as tasks, fix blocking bugs inline, journal
commentary, no scope reduction, switch when done/blocked.

Dependency order: advance-dd23-resolution → tighten-cmdsub-taint-execve;
regex-builtins → vet-var-modifiers; glob-splat Part 2 (shared word-interp
with vet); reject-undeclared / dd67-sugar / getpid independent.

## advance-dd23-resolution — starting

Hoist command resolution to a front-end pass (before validate/taint/
type-check) so the front-end sees absolute paths. Unresolved → fatal.
Key effect: the CST taint pass already taints `name.starts_with('/')`, so
resolved externals become correctly tainted with a span — fixes the
unblessed-external ICE and realizes Decision 5's taint goal (printf →
/usr/bin/printf → tainted).

## Reorder note

advance-dd23 turns out to be a coordinated cluster: `printf`/`pwd` are
`BuiltinCmd` entries that actually `execve`, so the internal-vs-external
(execve) classification is shared by advance-dd23 + tighten-taint +
getpid. Rather than half-build that, I'll do the clean self-contained
decided tasks first, then tackle the resolution/taint/getpid cluster as a
coordinated set. Starting with **dd67-sugar-sweep** (parser sugar).

## dd67-sugar-sweep — starting

Three ratified changes: re-add `name() {}` (→ fn sugar; currently SB4014);
re-add flat `` `cmd` `` (→ `$(cmd)`); single-bracket `[` becomes a normal
literal character (no `[ … ]` test; `[[ … ]]` only). `function name {}`,
`then…fi`, `do…done` already accepted.

### §1 name() {} — DONE

Added `parse_paren_function` + `is_paren_function` (WORD `(` `)` lookahead)
+ a dispatch guard; extracted `finish_function_def` shared with
`parse_function`. `name() { }` and `name() -> int { }` produce the same
`Function` IR as `fn name { }`. Specs functions 67.1-67.3 (all backends).
Big win: rust suite 180→152 — `name() {}` was unblocking ~34 tests.

### §2 flat backticks — DONE

New `Tok::Backtick(content)`: `scan_backtick` produces it for flat
`` `cmd` `` (→ `command_sub_to_expr` → CommandSubstitution, same as
`$(cmd)`). A `\`` nesting attempt (or unterminated) falls back to the
`` `backtick` `` sentinel the parser rejects ("nested backticks are not
allowed — use $(...)"). Split into scan_backtick/scan_backtick_body/
is_escaped_backtick for the complexity gate. Specs commands 67.4 (flat),
67.5 (nested rejected).

### §3 single-bracket `[` literal — VERIFIED (no code change)

Ratified keep/drop item B: single-bracket `[` is an ordinary literal
character (like backslash under DD#64), no longer a test construct;
`[[ … ]]` is the only test form and `arr[i]` subscript is unaffected.

The hand-parser already implements this — the tree-sitter-era single-
bracket test parsing did not survive the parser migration. Verified:
`echo [` → `[`, `x=[; echo $x` → `[`, `[[ a = a ]]` still tests,
`arr[i]` subscript still works. `[ a = a ]` compiles as the external
command `[`, resolved to `/usr/bin/[` (DD#23); no validator rejection.

Detour: `[ a = b ]` (exit 1) aborts the program before a following
statement. Confirmed this is **correct by DD#55** (status: implemented):
no runtime `set -e`, but the compiler inserts an implicit exit check on
every fail-able op — bare `cmd` aborts on nonzero; `- cmd` / `cmd ||` /
`if cmd` handle it. Not a bug. (Bare `false`/`true`/`:` run inline as
builtins and likewise abort on nonzero.)

Codified with specs commands 67.6a (`echo [ ]` → `[ ]`) and 67.6b
(`[ a = a ]; echo ok` → `ok`), both green in native + c + janet. No
existing spec regressed; native baseline held at 2 (cf.c7/cf.c8).

Keep/drop table keep-items (name(), backticks, single-bracket literal)
are all DONE. Remaining for the dd67 task: the **sash-convert
canonicalize switch** (task §3) — rewrite all sugar → native-only form.

## advance-dd23-resolution — DONE

Hoisted command resolution ahead of the front-end so the taint pass
classifies on the real exec form (gate 1 — the core fix: the CST taint
pass keys on `name.starts_with('/')`, so a bare `cat` was under-tainted;
`/usr/bin/cat` is correctly tainted).

New `sash/src/resolve.rs` (`resolve_commands(&mut Program)`): a mutating
front-end pass that walks every command head (statements, pipelines,
control-flow bodies, redirect targets, AND command-substitution heads via
the Expr walk) and rewrites each true-external literal head to its resolved
absolute path. Skip-set = path-heads, `BuiltinCmd::from_name != Other`,
the intrinsic list (getenv/argv/arg/argc/status/len/keys/join/epochseconds/
random/getpid/getppid/getcpid/getcwd/signum/dtoi/itod, kept in sync with
`ssa_taint::cmd_subst_taint`), user functions, and the `var =~ s/…/…/`
regex-mutation shape (mirrors `validate::is_regex_mutation`).

Wired into `cli_main` (lib.rs) between parse and the checks. Discovered the
front-end already had two relevant pieces, so this *consolidated* rather
than duplicated:
- **`validate::check_commands` (SB5011)** already rejected unresolved
  *statement* commands (but not cmdsub heads, and not sysroot-aware). Kept
  it; reused **SB5011** (not a new code) for the resolve pass's unresolved
  case, which additionally covers cmdsub heads.
- **`resolve_command_path`** (was `pub(super)` in transform) moved to
  `resolve.rs` (`pub(crate)`); `transform::resolve_pipeline_command` and
  `validate::resolve_on_path` now both call it — one sysroot-aware
  implementation. `validate::resolve_on_path` was NOT sysroot-aware before;
  now is.
- **squashfs** (gate 3) was already implemented: `lib.rs::sysroot_file_exists`
  reads `unsquashfs -l`. `--sysroot=<dir>` and `--sysroot=<image.squashfs>`
  both work; verified a command present only in a `--sysroot` dir resolves.

**Ordering fix (regression caught + fixed).** Initially resolution ran
before *all* front-end checks and fataled on the body lines a rejected
heredoc leaks as bare commands (`cat <<EOF\nhello…\nEOF` → `__HEREDOC__`
sentinel + leaked `hello`/`EOF` commands), pre-empting the heredoc
rejection (SB5002) with a spurious command-not-found. Split
`run_front_end_checks` into **`run_structural_checks`** (validate's
DD-level rejections: heredoc SB5002, export/declare SB5001, …) which now
runs BEFORE resolution, and **`run_semantic_checks`** (taint + positionals
+ argv-splat + type) AFTER. So an unsupported construct surfaces its own
diagnostic, and gate 1 (resolution before taint) still holds. Fixed
`neg_heredoc_cmdsub` + `brush_rejected_heredoc_cmdsub`.

Gates: (1) taint sees abs paths ✓ (2) unresolved = fatal compile error
SB5011, now incl. cmdsub heads ✓ (3) `--sysroot` dir + squashfs ✓ (4) the
runtime "bare command passed to sash_exec" is unreachable for resolved
paths ✓. Native spec baseline held at 2 (cf.c7/cf.c8); c/janet now also get
resolved abs paths (more DD#23-correct).

NB: per DG, the Rust suites (sash/tests/) have significant bit-rot and are
no longer a gate — spec.db is the source of truth. Filed
`audit-rust-test-suites`. The codegen tests call `compile_to_object`
in-process and bypass this pass entirely.
