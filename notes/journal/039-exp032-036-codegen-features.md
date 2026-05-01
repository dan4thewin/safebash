# String concatenation

Added `sash_concat(a_ptr, a_len, b_ptr, b_len) -> (ptr, len)` to sash-rt.
Allocates new buffer, copies both halves. Codegen emits concat for multi-part
string expressions (variable interpolation, mixed literal+variable).

Also added `sash_atoi(ptr, len) -> i64` and `sash_itoa(val) -> (ptr, len)` for
string↔integer conversion at the boundary (DD#16).

## While loops

The big challenge: SSA phi nodes in Cranelift.

### Key insight (after failed attempts)

All SSA versions of the same bash variable must share the **same** Cranelift
Variable pair (ptr, len). Cranelift's SSA builder handles block parameter
merging automatically — we don't need to emit explicit phi nodes. We just:
1. Define the variable in the entry block
2. Use `def_var`/`use_var` consistently across all blocks
3. Cranelift inserts block parameters at merge points

### Two-pass while lowering

Single-pass lowering failed: the loop body referenced pre-loop SSA versions
instead of phi-merged versions. Fix: two-pass approach in `transform.rs`:
1. **First pass**: scan body to discover which variables are modified, insert
   phi assignments at loop header
2. **Second pass**: re-lower body statements using phi-merged SSA versions

### $# initialization fix

`$#` was stored as `(null_ptr, count_as_len)` which broke numeric comparisons
(atoi of a null pointer). Fixed by adding `sash_itoa` and converting `$#` to
its string representation at program initialization.

## Numeric test comparisons

`[[ $x -lt 5 ]]`, `-eq`, `-gt`, `-ne`, `-ge`, `-le` — codegen calls `sash_atoi`
on both operands, emits Cranelift `icmp` with appropriate `IntCC` condition.

### Test expression interning fix

Literals inside `[[ ]]` weren't being interned, causing `lower_expr_value` to
return `(0, 0)`. Added `intern_test_expr` to the interning pass and
`scan_test_for_vars` to find variable references in test expressions.

## Tests

- 13 codegen tests (concat, while, numeric comparisons, argc numeric)
- 59 oracle tests (inherited + new parity tests)
- All passing

## Files changed

- `sash-rt/src/lib.rs`: `sash_concat`, `sash_atoi`, `sash_itoa`, `sash_arith_eval`
- `032/src/codegen.rs`: concat emission, while loop blocks, numeric icmp
- `032/src/transform.rs`: two-pass while lowering with phi insertion
- `032/src/lower.rs`: `intern_test_expr`, `scan_test_for_vars`
- `032/tests/codegen_tests.rs`: 13 integration tests

# Experiment 033: shift, export, $@ in codegen

**Goal**: Three small features needed for `~/bin/kusto` compilation target.

## shift

Codegen adjusts the global `sash_argc` and `sash_argv` at runtime:
decrement argc by 1, advance argv pointer by 8 bytes (one pointer).
After shift, all positional params (`$1`, `$2`, `$#`, `$@`) are
reloaded from the updated globals by iterating the vars map and
re-emitting loads for any variable whose base name is numeric, `#`,
or `@`.

## export

New SSA instruction `SsaInst::Export { target, value, name }` — emits
an assignment (same as `Assign`) followed by a call to `sash_setenv`
which updates the process envp array. `sash_setenv` either replaces an
existing entry (pointer swap) or allocates a new envp array with the
entry appended.

## $@

New runtime function `sash_join_args(argv, argc, sep_ptr, sep_len,
out_len)` joins `argv[1..argc]` with a separator. In echo context,
`$@` is initialized at startup by calling `sash_join_args` with a
space separator. After `shift`, it's re-joined from the updated argv.

Note: `"$@"` in command argument position (where each arg should be a
separate argv entry) is not yet handled — would need dynamic argv
construction at the call site. The current implementation handles the
echo/interpolation case only.

## sash-rt additions

- `sash_setenv(name_ptr, name_len, val_ptr, val_len)` — set env var
- `sash_join_args(argv, argc, sep_ptr, sep_len, out_len) -> ptr` — join args

## Tests

- 20 codegen tests (13 inherited + 7 new: shift_basic, shift_with_argc,
  export_basic, export_visible_to_child, dollar_at_echo, dollar_at_empty,
  shift_then_dollar_at)
- 59 oracle tests (inherited, all passing)

# Experiment 034: Redirections in codegen

**Goal**: Support `2>/dev/null`, `>&2`, and `> file` redirections.

## Approach

Added `redirects: Vec<Redirect>` field to `SsaInst::Command`. The
SSA transform now threads redirect info from `StmtKind::Redirected`
through to the last emitted Command instruction.

Codegen emits a save/redirect/restore pattern around each command:

1. **Save**: `dup(fd)` to preserve the original fd
2. **Redirect**: `sash_redirect(fd, path, flags)` for file targets,
   `sash_dup2(target_fd, fd)` for `>&N` fd duplication
3. **Execute**: the command runs with redirected fds
4. **Restore**: `dup2(saved, fd)` then `close(saved)`

Interning pass updated to intern redirect target paths (e.g.
`/dev/null`).

## sash-rt addition

- `sash_dup(fd) -> new_fd` — raw dup syscall

## Tests

- 23 codegen tests (20 inherited + 3 new: redirect_stderr_to_devnull,
  redirect_stdout_to_file, redirect_stderr_devnull_on_command)
- 59 oracle tests (inherited)

# Experiment 035: Command substitution codegen

**Goal**: Verify and test `$(...)` command substitution in assignment context.

## Findings

The full fork/pipe/exec/read pipeline for `SsaValue::CmdSubst` was already
implemented in codegen (from exp 028). The SSA transform correctly lowers
`X=$(cmd args...)` into `SsaInst::Assign { target: x_1, value: CmdSubst([cmd, args...]) }`.

Codegen builds a null-terminated argv on the stack, forks, redirects the
child's stdout through a pipe, reads into a 64KB heap buffer, strips the
trailing newline (shell convention), and returns (ptr, len).

## Tests added

7 new codegen tests:
- cmdsub_echo — basic `X=$(echo hello)`
- cmdsub_in_string — interpolation `"hello $NAME"` where NAME comes from cmdsub
- cmdsub_with_args — multi-arg command `$(echo -n foo)`
- cmdsub_multiline_strips_trailing_newline — confirms newline stripping
- cmdsub_empty_output — `$(true)` produces empty string
- cmdsub_in_conditional — cmdsub result used in `[[ -z ]]`
- cmdsub_with_redirect — confirms cmdsub works (redirect on outer cmd TBD)

**Totals**: 30 codegen + 59 oracle = 89 tests

# Experiment 036: Associative arrays (declare -A)

**Goal**: Support `declare -A MAP=([key]=val ...)` and `${MAP[$KEY]}` subscript access.

## Design

Associative arrays are stored as flat tables of `(key_ptr, key_len, val_ptr, val_len)`
tuples on the stack. The variable's StringVar (ptr, len) slots store (table_address,
entry_count) instead of the usual (string_ptr, string_len).

Lookup uses `sash_assoc_lookup` in sash-rt: linear scan comparing keys byte-by-byte.
For kusto's ~10-entry cluster map, linear scan is optimal.

## Implementation

**IR**: Added `ExprKind::AssocArray(Vec<(String, Expr)>)` and
`ExprKind::Subscript { name, index: Box<Expr> }`.

**Lower**: `lower_assoc_assignment` parses `[key]=value` concatenation nodes from
tree-sitter's array children. Expansion handler produces `Subscript` when a
`subscript` node is found (instead of flattening into `name[idx]` string).

**SSA**: `SsaValue::AssocArray(Vec<(String, SsaValue)>)` and
`SsaValue::Subscript { array, index: Box<SsaValue> }`.

**Codegen**: AssocArray builds the flat table on a stack slot, stores each entry's
key/value as interned data pointers. Subscript calls `sash_assoc_lookup`.

**sash-rt**: Added `sash_streq` (byte comparison) and `sash_assoc_lookup` (linear scan).

## Bug found and fixed

`scan_value_for_vars` was adding the array base name (e.g. "MAP") to
`positional_refs`, causing the init pass to create MAP_0 with fresh CL Variables
and initialize it via `getenv("MAP")`. This shadowed the AssocArray assignment
(MAP_1) which used different CL Variables from `base_vars`. Fix: don't add
Subscript array names to positional_refs — they're locally assigned variables.

## Tests

- 34 codegen tests (30 inherited + 4 new: assoc_array_basic, assoc_array_second_key,
  assoc_array_missing_key, assoc_array_variable_key)
- 59 oracle tests (inherited)

### 036 continued: Positional bounds checks, kusto integration, sash CLI

**Positional init bounds check**: `$1` init loaded `argv[1]` without checking
`argc`, segfaulting when called with no args. Fix: branch on `idx < argc` —
in-bounds loads the arg pointer + strlen, out-of-bounds sets (null, 0).

**Shift bounds check**: Same issue — shift reloaded positional params
unconditionally. Fix: same branch-on-bounds pattern.

**Quote stripping**: `declare -A MAP=([k]="value")` included literal quotes
in the stored value. Fix: strip matching quotes in `lower_assoc_assignment`.

**Redirected CmdSubst**: `TOKEN=$(az ... 2>/dev/null)` — the `command_substitution`
lowerer wraps the command in `StmtKind::Redirected`. The transform's CmdSubst
handler only matched `StmtKind::Command`, so it fell through to `SsaValue::Expr("$(..)")`.
Fix: unwrap `Redirected` to get the inner `Command` before matching.

**sash CLI**: Replaced the tree-sitter demo `main.rs` with a proper CLI:
`sash <script.sh> [-o <output>]`. Reads source, calls `compile_to_object`,
writes `.o`, links with `ld -static` against `sash-rt`, cleans up.

## Milestone: ~/bin/kusto compiled and running

The full `~/bin/kusto` script (42 lines) compiles to a **22KB static ELF** and
runs end-to-end:

```
$ sash ~/bin/kusto -o /tmp/kusto
$ /tmp/kusto azcore/OvlProd
>                                    ← Kusto CLI interactive prompt
```

The binary: resolves cluster alias via associative array, runs `az account
get-access-token` via command substitution, captures the JWT, validates it
with `-z` check, exports PATH, exec's `dotnet Kusto.Cli.dll` with the full
connection string. No libc, no interpreter, 22KB.

Features exercised: `declare -A`, `${MAP[$KEY]}`, `$1`, `shift`, `${SPEC%%/*}`,
`${SPEC#*/}`, `$(cmd)`, `if [[ -z ]]`, `if [[ $# -lt N ]]`, `echo ... >&2`,
`export`, `$@`, `exit`, string concatenation, env var lookup.

## Tests

- 44 codegen tests (10 new: kusto_cluster_resolve, kusto_usage_error,
  kusto_unknown_cluster, kusto_param_expansion, kusto_full_pipeline,
  kusto_full_unknown, kusto_full_usage_error, shift_then_dollar_at,
  redirect_stderr, export_visible_to_child)
- 59 oracle tests (inherited)

