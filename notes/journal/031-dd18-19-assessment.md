# Design Decision #18 (candidate): `local fn` — lexically scoped function shadowing

**Date:** 2025-04-15

**Problem:** Test harnesses need function mocking. Bash achieves this by
redefining functions at runtime, saving the original via `declare -f`,
and restoring via `eval`. Both `eval` and `declare -f` are incompatible
with ahead-of-time compilation.

**Resolution:** Introduce `local fn name() { ... }` — a function definition
scoped to the enclosing block. The shadow goes out of scope at `}`, and the
original definition is automatically restored. Same scoping semantics as
`local var`.

```bash
test_foo() {
    local fn real_func() { mock behavior; }
    # calls here see the mock
    call_something_that_uses real_func
}  # mock expires, original restored — no eval needed
```

The compiler knows at compile time which definition is active at each call
site — it's lexical, not dynamic.

**Eliminates:** `eval "$reset"`, `declare -f` for capture, `unset -f`.

**Grammar:** Needs tree-sitter-bash-safebash patch — `local fn` as a
declaration form in compound statements.

## Design Decision #19 (candidate): Runtime function selection via SSA phi

**Date:** 2025-04-15

**Problem:** Production scripts like `soc-os-update-flash.sh` redefine
functions at runtime based on version detection:

```bash
if [[ $ver -lt 201904 ]]; then
    dumpimage() { command dumpimage ... "$3" "unused"; }
else
    dumpimage() { command dumpimage ... "$3"; }
fi
```

Both definitions are visible in the source. The choice depends on a runtime value.

**Resolution:** Treat this as SSA — the compiler sees both definitions, emits
both as concrete functions (`dumpimage_v1`, `dumpimage_v2`), and uses a phi
node to select which one is active based on the runtime branch. Every call
site goes through the selector (indirect call or branch-selected dispatch).

Requirements:
- All branches must define the function (no "maybe defined" paths)
- All definitions must have the same arity
- The compiler validates these at compile time

This also handles the `check_soft_deps` eval pattern — `eval 'cpio() { ... }'`
inside an `if` becomes a static function definition in one branch, with the
external command path as the other branch. No `eval` needed; the source needs
minor rewriting to make branches explicit.

## Assessment: nothing fundamentally hard remains

**Date:** 2025-04-15

After analyzing three compilation targets (kusto, unittest,
soc-os-update-flash.sh), all remaining work falls into:

1. **Incremental compiler features** — same shape as 027–031:
   concat, arithmetic, while, case, arrays, heredocs, herestrings,
   `read -r`, `getopts`, `printf -v`, `command -v`, substring
   expansion, pattern substitution, array length, file tests,
   glob matching in `[[ ]]`, namerefs (DD#3)

2. **Composable from DD#14 primitives** — fork/exec/wait/pipe already exist:
   subshells, backgrounding (`&`), `kill` (one syscall), fd redirection
   codegen, pipelines

3. **Resolved by design decisions**:
   - `eval` for mocking → `local fn` (DD#18)
   - Runtime function redef → SSA phi (DD#19)
   - `. /etc/os-release` → `sash-expand` KVP loader (DD#1)
   - Type safety → `int` type system (DD#16, DD#17)

4. **Compile-time decisions** — `set -eu`, `shopt -s extglob/lastpipe/nullglob`

Only genuinely interactive constructs (`select`, coproc) remain out of scope.
None of the three targets use them.

