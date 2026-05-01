# Experiment 028: Parameter expansion operators

**Date:** 2026-04-13

**Goal:** Compile scripts using `${var%%pat}`, `${var%pat}`,
`${var##pat}`, `${var#pat}`, and `${var:-default}` — the parameter
expansion operators needed by `~/bin/kusto`.

**Changes to sash-rt (lib.rs):**
- `sash_strip_suffix(s, s_len, pat, pat_len, longest) → new_len`:
  Implements `%%` (longest=1) and `%` (longest=0). Handles glob
  patterns with `*` at start or end, plus literal suffix matching.
  Pointer unchanged; returns truncated length.
- `sash_strip_prefix(s, s_len, pat, pat_len, longest) → offset`:
  Implements `##` (longest=1) and `#` (longest=0). Same glob
  pattern handling. Returns byte offset to skip from start.
- `sash_default_value(s_len) → 0|1`: Returns 0 if empty (use
  default), 1 if set. Used by `:-` operator.

**Changes to SSA (ssa.rs):**
- Added `SsaValue::ParamExpansion { var, operator, operand }` variant
  to represent expansions with operators.

**Changes to transform (transform.rs):**
- `expr_to_ssa_value` now checks `ParameterExpansion` for an operator
  field. Plain `${var}` remains `SsaValue::Var`; expansions with
  operators become `SsaValue::ParamExpansion`.
- Added `expr_to_string` helper for operand extraction.

**Changes to codegen (codegen.rs):**
- `emit_ssa_value` handles `SsaValue::ParamExpansion`:
  - `%%`/`%`: calls `sash_strip_suffix`, returns (same ptr, new_len)
  - `##`/`#`: calls `sash_strip_prefix`, returns (ptr+offset, len-offset)
  - `:-`/`-`: calls `sash_default_value`, uses `select` to pick
    variable value or default operand
- Unset variables with `:-` operator treated as empty (len=0) instead
  of panicking — enables `${x:-fallback}` when x was never assigned.
- Interns operand strings alongside other literals.
- Added `strip_suffix`, `strip_prefix`, `default_value` to ExternFuncs.
- `scan_value_for_vars` handles `ParamExpansion` to pre-initialize
  positional/env vars used in expansions.

**Bugs found and fixed:**
- Glob direction was backwards in both strip functions — `starts_with(b"*")`
  and `ends_with(b"*")` were swapped. For suffix stripping, `/*` means
  "/ then anything" which is `*` at end of pattern. Same logic error
  in prefix. Both fixed.
- Original API used `out_len: *mut i64` pointer parameter for return
  values. Cranelift stack slot pointer passing didn't work (function
  was called but result ignored). Refactored to simpler return-value
  API: suffix returns new_len, prefix returns offset.

**Tests:** 10 end-to-end tests:
- `strip_suffix_longest` — `${x%%/*}` on `foo/bar/baz` → `foo`
- `strip_suffix_shortest` — `${x%/*}` on `foo/bar/baz` → `foo/bar`
- `strip_prefix_shortest` — `${x#*/}` on `foo/bar/baz` → `bar/baz`
- `strip_prefix_longest` — `${x##*/}` on `foo/bar/baz` → `baz`
- `default_value_empty` — `${x:-fallback}` when unset → `fallback`
- `default_value_set` — `${x:-fallback}` when set → value
- `strip_suffix_no_match` — no match returns original
- `kusto_style_spec_split` — `CLUSTER=${SPEC%%/*}` + `DB=${SPEC#*/}`
  on `azcore/mydb` → `azcore` + `mydb` (kusto's actual pattern)
- `regression_echo` — basic echo still works
- `regression_positional` — `$1` still works

All tests compare native binary output against bash.

---

## Experiment 029: Runtime command substitution

**Date:** 2026-04-13

**Goal:** Compile `x=$(command args...)` — runtime command substitution
where the command is an external program, not an elidable function.
This is the pattern kusto uses for `TOKEN=$(az account get-access-token ...)`.

**Design Decision #14: Composable process primitives.**

Instead of monolithic `sash_run` (fork+exec+wait) and `sash_capture`
(fork+pipe+exec+read+wait) wrappers, the runtime now exposes:
- `sash_fork() → pid` (0 in child, >0 in parent, -1 error)
- `sash_exec(argv, envp) → !` (execve with PATH lookup, no return)
- `sash_wait(pid) → exit_status`
- `sash_pipe(out_ptr) → 0|-1` (writes [read_fd, write_fd])
- `sash_close(fd) → 0|-1`
- `sash_dup2(src_fd, dst_fd) → 0|-1`
- `sash_redirect(fd, path, flags) → 0|-1` (open+dup2, for `2>/dev/null`)
- `sash_read(fd, buf, len) → bytes_read`
- `sash_alloc(n) → ptr` (bump allocator, now exported)

Redirects happen in the child's address space between fork and exec.
Parent's file descriptors are never touched. This makes `2>/dev/null`,
`>file`, `2>&1` all composable without special-casing each one.

Legacy `sash_run` and `sash_capture` are rebuilt as thin wrappers
on top of the primitives, preserving backward compatibility with
experiments 021–028.

**Changes to sash-rt (lib.rs):**
- Replaced monolithic `run_command()` with composable primitives
- `sash_alloc` promoted from internal to `#[no_mangle] pub extern "C"`
- `sash_capture` rewritten using `sash_fork`/`sash_pipe`/`sash_exec`/
  `sash_read`/`sash_close`/`sash_wait` (was ~100 lines of inline
  fork/pipe/exec, now ~30 lines calling primitives)
- `sash_run` reduced to 5 lines: fork → child exec / parent wait

**Changes to SSA (ssa.rs):**
- Added `SsaValue::CmdSubst(Vec<SsaValue>)` — carries the command's
  argv as SSA values (allows variable interpolation in arguments).

**Changes to transform (transform.rs):**
- `CommandSubstitution` that isn't an elidable function call now
  emits `SsaValue::CmdSubst` with argv extracted from the command.

**Changes to codegen (codegen.rs):**
- `lower_value` handles `SsaValue::CmdSubst`: emits the full
  fork/pipe/exec/read sequence using composable primitives:
  1. Build argv array on stack
  2. `sash_pipe` → get read/write fds
  3. `sash_fork` → branch on pid==0
  4. Child block: close read fd, dup2 write→stdout, close write, exec, trap
  5. Parent block: close write fd, read loop into heap buffer,
     close read fd, wait, strip trailing newline
- All 9 composable primitives declared as extern functions
- `intern_value` and `scan_value_for_vars` handle `CmdSubst`

**Tests:** 9 end-to-end tests:
- `cmdsub_echo_simple` — `x=$(echo hello)` → `hello`
- `cmdsub_uname` — `x=$(uname -s)` → matches bash
- `cmdsub_with_args` — `x=$(date +%Y)` → matches bash
- `cmdsub_in_echo` — `echo "$(echo world)"` → `world`
- `cmdsub_strips_trailing_newline` — shell convention verified
- `cmdsub_with_variable_arg` — `$(echo "$name")` with interpolation
- `regression_echo`, `regression_positional`, `regression_param_expansion`

---

