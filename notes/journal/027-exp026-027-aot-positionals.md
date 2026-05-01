# Experiment 026: End-to-end AOT compilation

**Date:** 2026-04-13

**Goal:** Unify the complete pipeline: bash source → parse → validate
→ IR → SSA → Cranelift codegen → link → static native binary. Prove
that all phases work together on a real script.

**Architecture:**

```
demo.sh → tree-sitter → CST
       → validator (7 rules) → diagnostics
       → lower → IR (12 statements)
       → transform → SSA (5 blocks)
       → codegen → object file (4312 bytes)
       → ld + libsash_rt.a → static ELF (16280 bytes)
```

**The unified `sash` binary:**
- `sash script.sh` — validates, compiles, links, produces a native binary
- `sash script.sh --emit-obj` — stops after object file
- `sash script.sh --skip-validate` — compiles without validation

**Demo script** (`demo.sh`) exercises all implemented features:
- Variable assignment and interpolation (`"$name v$version"`)
- Function definition + elidable command substitution (`$(get_status)`)
- For loop (compile-time unrolling over literal word list)
- If/else with `[[ ]]` string comparison
- External commands (`uname -s`, `date +%Y-%m-%d`)
- Multi-part double-quoted strings (`"$x $y"`)

**Output:** `diff <(./demo) <(bash demo.sh)` produces no differences.

**Binary properties:**
- 16,280 bytes, statically linked, no libc
- Pure Rust toolchain: tree-sitter + Cranelift + rustix
- Only external tool: `ld` (linker)

**Fixes applied:**
- `lower_string` gap fix: captures literal text between variable
  expansions in double-quoted strings (e.g., space in `"$x $y"`)

**Tests:** 12 end-to-end tests covering:
- Compilation + bash comparison (echo, assign, if/else, for, function cmdsub, external cmd, mixed script)
- Validator integration (rejects eval, backticks, indirect expansion, single bracket)
- Binary properties (statically linked, under 1MB)

**Conclusion:** The safebash pipeline is complete from source to
native binary. A bash script goes in, a verified static ELF comes
out. The binary produces identical output to bash. All implemented
language features work end-to-end through all six compilation phases.

---

## Experiment 027: Positional parameters, env vars, exit builtin

**Date:** 2026-04-13

**Goal:** Compile scripts that access `$1`, `$2`, `$HOME`, `$USER`,
and use `exit N`.

**Changes to sash-rt:**
- Added `sash_argc` and `sash_argv` globals, populated by `_start`
- Added `sash_getenv(name, name_len, &out_len) → ptr` for env vars
- Added `sash_capture(argv, envp, &out_len) → ptr` for runtime
  command substitution (fork/exec with stdout capture)
- Added bump allocator for capture buffers (128KB, no-op free)
- Updated `_start` to store argc and argv for both x86_64 and aarch64
- Added `pipe` feature to rustix dependency

**Changes to codegen:**
- Pre-scans SSA for variable references that are positional params
  (numeric names), special vars (`#`, `@`), or env vars (ALL_CAPS)
- Initializes positional params from `sash_argv` at block entry
- Initializes env vars via `sash_getenv` call
- Handles `exit N` as builtin: calls `sash_exit`, traps, and
  signals block termination to skip remaining instructions
- Handles `shift` as no-op (TODO: implement properly)
- Imports `strlen` from sash-rt for null-terminated string length
- Added `sash_capture`, `sash_getenv`, `strlen`, `sash_argc`,
  `sash_argv` to extern declarations

**Tests:** 10 end-to-end tests: positional params, env vars, exit
builtin (code 0 and 42), mixed positional+literal, regressions.

**Binary size:** 11,400–11,504 bytes (still under 16KB).

---

