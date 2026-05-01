# Experiment 021: Cranelift hello world

**Date:** 2026-04-12

**Goal:** Prove we can generate a native binary from Rust using
Cranelift's code generation API.

**Actions:**

Built a program that:
1. Creates a Cranelift IR function (`main`) that calls libc `write(1,
   buf, 12)` to print "hello world\n" then calls `_exit(0)`.
2. Emits a `.o` object file via `cranelift-object`.
3. Links it with `cc -no-pie` to produce a native ELF binary.
4. Runs the binary, asserts stdout = "hello world\n" and exit 0.

**Issues encountered:**

- `TrapCode::user(0)` returns `None` — user trap codes must be
  non-zero. Fixed to `user(1)`.
- `InstBuilder` trait not in scope — Cranelift 0.130 requires
  explicit `use cranelift_codegen::ir::InstBuilder`.
- Initial attempt used `-nostartfiles -static` linking, which fails
  on Ubuntu 24.04 due to missing `_init`/`_fini`/`_DYNAMIC` symbols.
  Switched to `cc -no-pie` using standard libc startup (defines
  `main` instead of `_start`).

**Result:** Native ELF x86-64 binary generated and executed
successfully. Two tests pass: IR verification and end-to-end binary
execution.

**Cranelift version:** 0.130.1

**Key dependencies:**
- `cranelift-codegen` — IR types, code generation
- `cranelift-frontend` — `FunctionBuilder` API for constructing IR
- `cranelift-module` — module-level linking (data + functions)
- `cranelift-native` — host ISA detection
- `cranelift-object` — emit to object files

**Conclusion:** Cranelift works. Next experiment: lower SSA
assignments to Cranelift IR (string variables as heap values,
assignment as store, expansion as load).

---

## Experiment 022: Cranelift string variables and echo

**Date:** 2026-04-12

**Goal:** Compile bash variable assignments and echo to native code
via Cranelift. Full pipeline: bash → parse → IR → SSA → Cranelift
→ object → link → run.

**Actions:**

1. Built `codegen.rs` — lowers SSA to Cranelift IR:
   - String variables are (ptr, len) pairs. Literals live in the
     data section. Assignment copies the pair. Variable reads use
     `use_var` to get the current (ptr, len).
   - Echo calls `write(1, ptr, len)` for each argument with spaces
     between and a trailing newline.
   - Program exits via `_exit(0)`.

2. `compile_to_object()` — convenience function that runs the full
   pipeline: parse → lower → SSA → Cranelift → object bytes.

3. Fixed SSA transform: `DoubleQuoted([Literal("hello")])` was
   being flattened to `Expr("\"hello\"")` instead of unwrapping to
   `Literal("hello")`. Same for `DoubleQuoted([SimpleExpansion("x")])`
   → should be `Var(x_1)`. Single-element DoubleQuoted now unwraps;
   all-literal multi-part concatenates.

4. Fixed test race condition: all tests were writing to the same
   `/tmp/sash_test` path. Now each test gets a unique path via
   atomic counter.

**Cranelift API discovery:**
- Cranelift 0.130: `declare_var(Type) -> Variable` (no more
  `Variable::new(index)`). The builder allocates variable indices.
- `InstBuilder` trait must be explicitly imported.
- `TrapCode::user(0)` returns `None` — user codes must be non-zero.

**Tests:** 10 end-to-end tests:
- echo_literal, echo_quoted_literal, echo_multiple_literals, echo_no_args
- assign_and_echo, assign_two_vars_echo, reassign_variable
- assign_empty_string, multiple_echo_statements
- compile_produces_nonempty_object

**Conclusion:** The pipeline works end-to-end: bash source →
native binary → correct output. Variables are heap strings, echo
calls write(). Next: fork/exec for external commands (exp 023).

---

## Experiment 023: External command execution via fork/exec

**Date:** 2026-04-12

**Goal:** Compile non-builtin commands (`date`, `uname`, `ls`, etc.)
to native code that forks and execs the external program.

**Actions:**

1. Wrote `sash_rt.c` — a small C runtime that provides `sash_exec()`:
   - Takes parallel arrays of (ptrs, lens, argc)
   - Copies each (ptr, len) pair to a null-terminated string
   - Builds argv, forks, calls execvp in the child
   - Parent waitpids and returns the exit status

2. Extended `codegen.rs`:
   - Declares `sash_exec` as an imported function
   - `lower_exec()`: builds two stack-allocated arrays (pointers
     and lengths), stores command name + args, calls sash_exec
   - `lower_inst()`: dispatches echo to write(), true/false to
     no-ops, all other commands to sash_exec

3. Refactored codegen into free functions (`lower_inst`, `lower_echo`,
   `lower_exec`, `lower_value`) instead of methods on a struct — the
   closure-based approach from 022 didn't scale with multiple function
   references.

**Design decision:** Use a C runtime helper rather than emit
fork/exec syscalls directly in Cranelift IR. Keeps the IR clean
and the runtime auditable. The runtime is linked into the binary
as `sash_rt.o`.

**Tests:** 12 end-to-end tests:
- echo regression (literal, assign+echo)
- External commands: date, uname, /bin/echo, printf, cat, wc
- Multiple commands in sequence
- External command with variable argument
- touch + ls
- Object file size check

Each test compiles bash → Cranelift → object, links with sash_rt.o,
runs the binary, and asserts correct stdout.

**Conclusion:** External commands work via fork/exec. The compiled
binary can run arbitrary programs. Next: control flow (exp 024).

---

