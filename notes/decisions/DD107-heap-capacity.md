title: "DD#107 — Heap capacity: a 256 MB compiled-in default, set by `--heap-size`"
status: accepted

1. The runtime heap is one fixed reservation, defaulting to 256 MB with
   `MAP_NORESERVE`; physical pages are committed as touched.
2. `--heap-size=SIZE` selects the compiled-in capacity using a byte count
   or `K`/`M`/`G` suffix. Runtime environment variables do not set it.
3. Allocator exhaustion is a diagnosed fatal fault, not a resumable
   result or a successful partial allocation. DD#128/DD#130 govern its
   allocation-free terminal dispatch.

---

The runtime heap is a single fixed region. It reserves **256 MB** by
default, mapped `MAP_NORESERVE` so pages are committed only when touched.
`sash --heap-size=<size>` changes the reservation at compile time; the
size accepts a plain byte count or a `K`/`M`/`G` suffix.

The size is **compiled in and never read from the environment.** There is
deliberately no `$SASH_HEAP`. safebash goes out of its way to keep the
environment from steering a running binary — that is what the taint rules
are for, and why `$HOME` must be written `$(getenv HOME)` — so a heap size
read from the environment would be a way back in: it would let anything
that controls the environment shrink a binary's heap until it aborts, or
inflate it, without touching the program. Baking it in keeps the binary's
memory behaviour a property of the artifact.

`--trace` (DD#32) is the model for the exception, not a counterexample:
tracing is opted into *at compile time*, and `SASH_TRACE` can only
activate machinery the build already chose to include. A binary compiled
without `--trace` ignores the variable entirely.

## Why a ceiling at all

heap_4 (DD#26) has one `xStart`/`pxEnd` sentinel pair and cannot span
discontiguous mappings, so the reservation is a hard limit rather than a
starting size. **This is a known limitation.** Exceeding it is a clean
`sash: out of memory (requested N bytes)` on stderr with exit 1 — never a
segfault (that half shipped 2026-09-12). Growing the heap on demand across
multiple regions is filed as `heap-multi-region-growth`.

## Why 256 MB

`NORESERVE` makes the reservation cost address space rather than memory:
a hello-world binary's peak RSS is 340 kB with the 256 MB default, the
same as it was under the old 1 MB one. The number is arbitrary in the
sense that any distant ceiling would do; it is chosen to be far past what
a shell script plausibly needs while staying trivially affordable on the
64-bit targets (x86_64, aarch64 — there is no 32-bit target).

Measured: the old 1 MB heap ran out at ~5 000 30-byte array elements.
The default now absorbs 200 000, and `--heap-size` moves the ceiling in
both directions — `--heap-size=1M` reproduces the old failure exactly.

## Mechanism

~~`heap4.c`~~ defines `sash_heap_size` **weakly** at 256 MB. When
`--heap-size` is given, the compiler emits a **strong** definition of the
same symbol into the object it generates — a Cranelift data object in the
native backend, a C global in `--emit-c` — and the linker prefers it. An
ordinary build emits no such symbol and picks up the weak default, so the
flag costs nothing when unused and needs no runtime branch.

DD#129 moves the runtime to Rust; the filename above records the earlier
C implementation, not a current source location.

## Specs

`hp.1`-`hp.3` (exhaustion is a clean abort, pinned against an explicit
1 MB heap rather than the default), `hp.6` (the default absorbs 50 000
elements), `hp.7` (`--heap-size` raises the ceiling) in `spec/misc.sh`;
`cli.13`, `cli.20`-`cli.23` (flag advertised, operands validated) in
`spec/cli.sh`.
