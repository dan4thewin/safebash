# Design Decision #26: FreeRTOS heap_4 allocator, compiled from C

**Date:** 2025-04-16

**Problem:** The current bump allocator (128KB, never frees) works for
short-lived scripts but fails for:
- Loops with command substitution (each iteration leaks the old value)
- Long-running services (memory grows without bound)
- Any script that reassigns variables in a loop

**Resolution:** Adopt FreeRTOS heap_4 as `sash_heap.c`. Address-ordered
free list with first-fit allocation and coalescing on free. The compiler
emits `sash_free(old_ptr)` at every variable reassignment and scope exit.

### Why heap_4

- Battle-tested: deployed on billions of FreeRTOS devices for 10+ years
- Audited, fuzzed, formally analyzed — the algorithm is proven correct
- Coalescing prevents fragmentation for long-running alloc/free patterns
- Simple: ~300 lines of C, single-threaded (no locking needed)
- Static heap array — no mmap, no sbrk, no OS allocator dependency

### Why C, not a Rust rewrite

Rewriting introduces risk. Every rewritten line diverges from the proven
original. A pointer cast error, alignment bug, or coalescing edge case in
the rewrite would introduce exactly the kind of memory corruption safebash
is designed to prevent. The algorithm stays untouched — only the FreeRTOS
wrappers are stripped.

Build impact: requires `cc` in the toolchain. Trade-off accepted —
`cc` is universally available, and algorithmic correctness outweighs
toolchain purity.

### Why not arena-per-scope

Arenas have zero per-allocation overhead and bulk-free simplicity, but:
- Loops inside a scope accumulate without freeing — grows forever
- Values that escape a scope need promotion (escape analysis or copy)
- Arena-per-iteration requires the compiler to identify scope boundaries
  for freeing, which is equally bug-prone as emitting free calls
- "Arenas can't have use-after-free" is only true if scope boundaries are
  correct — the compiler is the same codebase either way

### Codegen changes

The SSA already knows the lifetime of every value. Codegen is mechanical:
- Variable reassignment: `sash_free(old_ptr)` then assign new value
- Scope exit (function return, block end): `sash_free` all locals
- Function return value: caller owns it, frees on reassignment
- No escape analysis needed — one variable owns one allocation, no aliasing

### Implementation plan

1. Strip FreeRTOS-specific parts from heap_4.c into `sash_heap.c`
   (remove `vTaskSuspendAll`, `configASSERT` macros, hooks)
2. Configure: heap size (default 1MB?), alignment (8 bytes)
3. Export `sash_alloc(size) -> *mut u8` and `sash_free(ptr)`
4. Compile with `cc -c -O2 -ffreestanding`
5. Link alongside `libsash_rt.a` and Cranelift output
6. `sash_alloc`/`sash_free` in sash-rt become thin extern wrappers
7. Update codegen to emit `sash_free` at reassignment/scope-exit points
8. Tests: loop allocation, long-running simulation, fragmentation

### Aspiration

If safebash handles memory correctly, it becomes a viable alternative to
Go or Rust for long-running processes — daemons, systemd services,
monitoring loops. The allocator is one of the foundations that makes
this possible.

# Experiment 032: Concat, while loops, numeric tests in codegen

**Goal**: Three codegen features needed for real scripts — string concatenation,
while loops with SSA phi nodes, and numeric test comparisons (`-lt`, `-eq`, etc.).

