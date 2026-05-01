# Design Decision 12: Null-terminated counted strings

**Date:** 2026-04-12

**Context:** Experiment 023 uses a C runtime (`sash_rt.c`) to convert
(ptr, len) strings to null-terminated strings for `execve`. This is
wasteful and introduces a copy at every syscall boundary.

**Decision:** All strings in the compiled binary are stored as
`(ptr, len)` where `ptr` points to `len+1` bytes with `ptr[len] == 0`.
The `len` field is authoritative for content boundaries. The trailing
null is always present for C/syscall interop and defense in depth.

This is the same representation used in DG's Forth project: counted
strings with a hidden null terminator.

**Consequences:**
- String literals in the data section include the null byte.
- `write(fd, ptr, len)` uses `len` — ignores the null. Correct.
- `execve(path, argv, envp)` passes `ptr` directly — no copy needed.
- Concatenation: `malloc(len_a + len_b + 1)`, copy A (len_a bytes),
  copy B (len_b + 1 bytes including its null).
- The C runtime (`sash_rt.c`) is eliminated — no need for the copy
  step that was its reason for existing.
- Combined with inline syscalls, the compiled binary has zero
  external dependencies (no libc, no dynamic linker).

**Relation to other decisions:** This enables the move from libc
calls to inline Linux syscalls for `write`, `fork`, `execve`,
`wait4`, and `_exit`. The binary becomes a static, self-contained
ELF — strengthening the chain-of-trust story (fewer dependencies
to verify, complete syscall set visible in the IR).

**Implementation:** Experiments 022a and 023a will implement this
approach, replacing the libc-based 022/023. Experiments 022 and 023
are preserved as-is for historical reference.

---

## Experiment 022a: Null-terminated counted strings (v2)

**Date:** 2026-04-12

**Goal:** Implement Decision #12 — all strings stored as (ptr, len)
where ptr points to len+1 bytes with ptr[len] == 0.

**Changes from 022:**
- `intern()` appends a null byte to every literal in the data section.
- `lower_value()` reports `len` (content length, not including null).
- No change to echo/write — `write(fd, ptr, len)` uses `len`.
- New test: `string_data_is_null_terminated` verifies the object file
  contains `"hello\0"`.

**Tests:** 11 (10 from 022 + 1 null-termination check). All pass.

---

## Experiment 023a: Fork/exec via raw syscalls (v2)

**Date:** 2026-04-12

**Goal:** Replace the C runtime (sash_rt.c) with raw Linux syscalls.
No libc dependency. Truly standalone ELF binaries.

**Architecture:**
- `sash_syscall.S` — 5 syscall stubs (write, fork, execve, wait4,
  exit) plus `_start` entry point that saves `envp` to a global.
- `sash_path.c` — PATH lookup using only the syscall stubs. No libc.
  `sash_run(argv, envp)` handles fork, PATH search, execve, wait4.
  Compiled with `-ffreestanding -nostdlib`.
- `codegen.rs` — builds null-terminated argv directly from data-section
  pointers (no string copying thanks to Decision #12). Calls
  `sash_run(argv, envp)` where envp comes from the global set by
  `_start`.
- Linked with `ld` (not `cc`). No `-lc`. No dynamic linker.

**Key result:** `binary_is_statically_linked` test confirms `ldd`
reports "not a dynamic executable."

**Tests:** 13 end-to-end tests. Echo regression (4), external
commands with PATH lookup (date, uname, /bin/echo, printf), mixed
sequences, variable arguments, touch+ls, static linking check,
null-termination check. All pass.

**Note:** sash_path.c is technically C compiled without libc. It
uses only sash_syscall.S stubs and inline asm for access(). This
could be rewritten in Rust with `#[no_std]` or as more asm stubs.
The important thing is: zero libc in the final binary.

**Conclusion:** Compiled safebash binaries are now self-contained
static ELF executables with no external dependencies. The complete
set of syscalls is: write, fork, execve, wait4, exit, access.

---

## Experiment 023b: Pure Rust runtime via rustix

**Date:** 2026-04-12

**Goal:** Replace the C runtime (sash_rt.c) and assembly stubs
(sash_syscall.S) with pure Rust. No C, no external .S files.

**Architecture:**
- `sash-rt/` — a standalone Rust crate (`#![no_std]`, staticlib)
  that provides the runtime for compiled safebash binaries:
  - `sash_write(fd, buf, len)` — via `rustix::io::write`
  - `sash_exit(status)` — via `rustix::runtime::exit_group`
  - `sash_run(argv, envp)` — fork/execve/wait4 with PATH lookup,
    all via rustix syscalls
  - `_start` — 8 lines of inline asm per architecture (the ONLY
    assembly in the project). Extracts envp from the stack, calls
    main(), exits.
  - libc stubs (strlen, memcpy, memset, memcmp, bcmp) — ~5 trivial
    functions required because `no_std` doesn't provide them.
  - Panic handler writes "sash: panic" to stderr and exits 99.

- `exp/023b-rustix-runtime/` — test harness. Compiles bash → 
  Cranelift → .o, links with `ld` against `libsash_rt.a`, runs
  the resulting static ELF binary.

**Key metrics:**
- Binary size: 11,320 bytes for "echo hello"
- Static: `ldd` reports "not a dynamic executable"
- No libc, no C compiler, no assembler in the build
- Only dependency: rustix (by the Wasmtime/Cranelift team)

**Design decisions recorded:**
- Decision #12: null-terminated counted strings
- Decision #13: keep `ld` as the linker (battle-tested, audited,
  provides a verification boundary between .o files)
- `_start` is the only assembly — one `#[cfg(target_arch)]` block
  per architecture (~8 instructions each). Everything else is Rust.
- rustix with `default-features = false` for true no_std.

**Architecture portability:**
- x86_64: implemented and tested ✅
- aarch64: `_start` implemented, untested (needs aarch64 host)
- arm, riscv64: future — add one cfg block each (~10 lines)
- macOS: rustix falls back to libSystem backend; `_start` not
  needed (use `main` entry point via cc linkage)

**Tests:** 13 end-to-end: echo (5), external commands (5),
binary properties (3: static linking, size, null termination).

**Conclusion:** The safebash toolchain is now 100% Rust (plus ld).
No C, no external assembly files. Compiled binaries are self-
contained static ELFs using ~6 Linux syscalls: write, fork,
execve, wait4, exit_group, access.

---

