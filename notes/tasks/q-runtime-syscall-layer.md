---
title: "Question: what should sash-rt's syscall layer be? (rustix, vendored nolibc, or our own primitive)"
status: open
priority: P1
kind: question
depends: []
gates:
  - "DG chooses the runtime's syscall layer; the choice and its reasons are recorded in a new DD"
  - "the chosen remedy closes bug-fresh-clone-rustix-runtime-private, by that task's own gates"
  - "sash-rt/src/lib.rs:3-4 and docs/GUIDE.md:470 describe the layer actually in use"
  - "whatever the choice leaves for later is filed in cross-compile-and-setjmp-on-mac.md"
---

Captured from a discussion with DG on 2026-10-26, during the batch-531
port. DG: "capture all of this for further discussion into a backlog item;
keep working the port as-is for now". Nothing here is decided. The
recommendations are mine and unruled. Measurements are against ec45535.

## How it started

A fresh clone cannot build sash-rt (bug-fresh-clone-rustix-runtime-private,
P1). Cargo resolves rustix 1.1.5, which made the `runtime` module private,
and sash-rt calls three functions from it. Both lockfiles here pin 1.1.4,
so nothing is broken on this machine. Asking how to fix that became asking
what the runtime's syscall layer should be at all, with DG's targets in
view: "sash should ultimately be able to cross compile for arm, raspbery
pi, and mac os (perhaps linking for that last)".

## What sash-rt does today

The header (sash-rt/src/lib.rs:3-4) says "Uses `rustix` for all syscalls.
The only `asm!` is _start". Neither is true any longer:

- rustix's public API: about 90 lines in 26 functions.
- rustix's `runtime` module: `exit_group` (lib.rs:436), `kernel_fork`
  (653-658) and `execve` (693). These are what 1.1.5 breaks.
- Hand-written x86_64 syscalls, not gated by `cfg(target_arch)`:
  `fcntl(F_DUPFD_CLOEXEC)` (208), `dup2` (844), `close` (899), `getcwd`
  (3290), `rt_sigaction` in `sash_trap` (3493), `open` and `getdents64`
  in `sash_glob` (3660, 3685), and the `rt_sigreturn` restorer
  (3536-3540). The comment at 200-202 says the fd plumbing is
  syscall-level on purpose.
- `_start`, per architecture: x86_64 and aarch64 (1572-1610).
- `csrc/setjmp.S`, x86_64 only, for one caller (see setjmp below).
- docs/GUIDE.md:470 repeats "Uses `rustix` for raw Linux syscalls".

So sash-rt cannot build for another architecture today: the aarch64
`_start` is the only non-x86_64 code. (Inferred from the source; this host
has only the x86_64 Rust target, so it was not tried.)

## Why rustix hides fork, execve, exit_group and sigaction

From the rustix 1.1.5 source and tracker:

- `src/not_implemented.rs` puts `fork`, `clone`, `exit`, `sigaction`,
  `sigprocmask` and `pthread_atfork` under `libc_internals`, permanently
  out of scope. libc "expects to be the only thing in the process": it
  owns the atexit list, the pthread_atfork list, threads and signal
  handling. `execve`, `execveat`, `exit_group`, `_exit` and `_Exit` are
  under `quite_yet`: "their API might need more work and/or they don't yet
  have a libc backend implementation". `vfork`, `setjmp`, `longjmp` and
  `sigreturn` are `impossible`.
- The runtime module's docs say it is "for libc-like runtime libraries
  such as Origin" and "cannot be used in a process which also has a libc
  present". `kernel_fork` runs no atfork handlers and "does not update the
  thread id in the thread runtime". `execve` takes raw pointer arrays,
  "convenient for c-scape".
- `src/lib.rs:308-311`: the module name is mangled "with a random string
  that will rotate periodically", as a "counter-measure".
- bytecodealliance/rustix#1314, "Document and stabilize functions from
  rustix::runtime", open since 2025-02-06. It says a raw sigaction next to
  libc "may invoke undefined behavior", under conditions that "could
  change with libc versions".
- bytecodealliance/rustix#1133, "Add rustix::process::exit" (a public
  exit_group): open since 2024-08-26, untouched since 2025-04-09.
- bytecodealliance/rustix#1633, the rename: merged 2026-09-16 and released
  in 1.1.5, after "a few bug reports from users appearing to use
  rustix::runtime as if it were a normal library feature". On 2026-10-26 a
  user in that thread said they pin 1.1.4 for fork. sunfishcode answered
  that the libc bindings are more appropriate for fork "even if the rustix
  bindings were public".

Their objection is about sharing a process with libc. Ours has none (next
section), so the hazard does not apply to us. But rustix will not support
the use, and says so.

origin, the sanctioned consumer, is broken by the same rename. origin
0.26.2 (latest, 2025-05-15; main unchanged since 2026-02-18) asks for
rustix ^1.0.0. It fails with E0603 x9 against 1.1.5 and builds against
1.1.4 (measured).

## Our programs carry no libc (measured)

- Native: `ld -static --gc-sections` with only the object and
  libsash_rt.a (sash/src/lib.rs:1005-1016). No libc on the line.
- C mode (spec/common.sh:189): `gcc -nostartfiles -static ...
  "$SASH_RT_LIB" -lpthread`, so libc.a and libpthread.a are on the line.
  A program with a pipeline, a background job, a capture, a regex and `cd`
  pulled no member from either: the link map shows only their `LOAD`
  lines. A control that called `getppid` pulled `libc.a(getppid.o)`, so
  the map does show a pulled member when there is one.

Loose end: in C mode, an accidental libc reference from emitted C or from
the runtime links silently. Native fails at link, so a dual-mode sweep
still catches it, but an `--emit-c` user would not. Question 4 below.

## What rustix's public API can replace

Without alloc, it covers five of the hand-written sites:

| sash-rt | line | public rustix |
|---|---|---|
| `sash_fcntl_dupfd_cloexec` | 208 | `io::fcntl_dupfd_cloexec` |
| `sash_dup2` | 844 | `io::dup2` |
| `sash_close` | 899 | `io::close` |
| `sash_glob` (open) | 3660 | `fs::open` |
| `sash_glob` (getdents64) | 3685 | `fs::RawDir` (cfg linux_kernel) |

Two caveats:

- On aarch64 and riscv64, rustix's `dup2` is `dup3`, which fails with
  EINVAL when the two fds are equal. It also takes `&mut OwnedFd`. Our
  glue must handle old == new.
- aarch64 has no `open`, `dup2` or `fork` syscall at all (linux-raw-sys
  0.12.1). rustix uses `openat` and `dup3` there, which is why the x86_64
  asm cannot simply be re-spelled per architecture.

`getcwd` is the exception. The public one is
`cfg(all(feature = "alloc", feature = "fs"))` (process/chdir.rs:53) and
returns a `CString`. sash-rt has no alloc crate and no allocator.

So rustix will not give us five calls: fork, execve, exit_group,
rt_sigaction and getcwd.

## Options considered

- **R-pin**: `rustix = "=1.1.4"`. A dead end: frozen on one release, and
  the module stays unsupported.
- **R-origin**: use origin. Broken by the same rename (above).
- **Static musl**: reverses the no-libc property, which no DD decides.
  musl's fork, errno and sigaction depend on musl's own startup (its
  `__pthread_self()` and `libc.*` state), so it is all or nothing, and our
  `_start` goes. DG pointed out that any machine code gcc can compile can
  be embedded in our runtime library (build.rs already compiles csrc/ into
  libsash_rt.a). That removes the "libc.a at every sash link" cost, but not
  the all-or-nothing one. musl drops out. Its standalone setjmp and
  restorer files are not needed either (see setjmp and signal return).
- **(N) vendored nolibc for the five** (my recommendation). The kernel's
  tools/include/nolibc, as read 2026-10-26: LGPL-2.1 OR MIT, about 8900
  lines, 15 architecture headers including arm and Thumb.
  - Has: fork (via clone), execve, dup2 (dup3 where needed), close, open
    (always openat), getcwd, getdents64.
  - Lacks: sigaction, setjmp, a public fcntl. Its `_exit` is `__NR_exit`,
    not exit_group. rt_sigaction and exit_group would go through its
    syscall macros.
  - Its functions are `static`, so we export C wrappers. It needs the
    target's UAPI headers (linux-libc-dev; present here). Linux only.
  - The kernel tests it under QEMU on arm64, arm and armthumb
    (tools/testing/selftests/nolibc/run-tests.sh).
- **(R) our own Rust per-architecture primitive**: one small syscall
  function per architecture, with numbers from linux-raw-sys, possibly
  adapted from rustix's MIT/Apache code. Close to (N). The choice between
  them is whether we would rather own Rust asm or vendor C headers.
- Seen, not vetted: the `syscalls` crate (0.8.1, 2026-02-02).

In every option the ordinary calls move to rustix's public API, and only
`_start` and the x86_64 restorer stay hand-written. Using all of nolibc in
place of rustix would cost macOS (below), which is why (N) covers only the
five.

## setjmp

One use: Pike's regcomp error exit. `rcerror` longjmps
(sash-rt/csrc/regcomp.c:115) to `regcomp1`'s setjmp (693), which frees and
returns NULL. regcomp.c is built four times: the runtime, the compiler
(sash/build.rs, literal-pattern validation), sash-regex and test_regex
(mkfile). Only the runtime build lacks a libc setjmp, and its
csrc/setjmp.S is x86_64 only. In the runtime, `regerror9`
(lib.rs:2382-2402) prints and returns, and every `regcomp9` caller exits 2
on NULL. So at run time the jump fires only for a bad interpolated
pattern.

Options:

- **(A)** plain error returns in regcomp.c, no setjmp anywhere (my
  recommendation). 10 of the 18 `rcerror` sites already return or break
  (72, 99, 244, 532, 565, 572, 577, 598, 612, 732). 8 carry on (128, 157,
  160, 182, 191, 397, 431, 793).
- **(B)** the runtime's `regerror9` exits instead of returning. Needs a
  build-flag fork, and forecloses q-dynamic-regcomp-levers.
- **(C)** musl's per-architecture setjmp.
- **(D)** `__builtin_setjmp`. Ruled out: clang 20 rejects it on aarch64,
  and the GCC manual steers user code to libc.

## Cross-compile and macOS facts gathered

- Signal return: only x86_64 needs `SA_RESTORER`. arm64 returns through
  the vDSO's sigtramp, 32-bit arm through the kernel's signal page. The
  restorer stays an x86_64 special case under any option.
- Cranelift 0.130.2 has no 32-bit ARM backend. A 32-bit Raspberry Pi is
  reachable only through `--emit-c` and a cross gcc. 64-bit Pi OS is
  aarch64.
- 41 hard-coded `CallConv::SystemV` sites must take the ISA's default
  (SystemV, or AppleAarch64 on macOS).
- macOS links libSystem dynamically under any option, and our `_start`
  goes. It has no getdents64, so glob would use opendir/readdir. Under
  rustix its libc backend covers the ordinary calls, and the five come
  from libc. Compiler side: Mach-O output, the AppleAarch64 call
  convention, the `.sash_source` section, the link step.
- This host has no aarch64 or arm toolchain, sysroot or qemu. Nothing
  above was run on another architecture.

## Proposed P1 scope, if (N) or (R) is chosen

Move fork, execve and exit_group off the runtime module. Drop sash's dead
rustix dependency (sash/Cargo.toml:14 is its only mention; the root
workspace builds without it). Fix lib.rs:3-4 and GUIDE:470. Write a DD for
the runtime's syscall policy. The rest (the other asm sites, setjmp,
cross-compile, macOS) goes to cross-compile-and-setjmp-on-mac.md.

## Questions for DG

1. Which layer for the five calls: (N), (R), or something else?
2. setjmp: (A)?
3. Should the P1 stay minimal as above, or move every asm site at once?
4. Should C mode link with `-nostdlib`, so that it fails like native on a
   stray libc reference? Is `-lpthread` still needed at all?
5. Which cross-compile goal comes first? (The three readings in
   cross-compile-and-setjmp-on-mac.md, question 1.)
