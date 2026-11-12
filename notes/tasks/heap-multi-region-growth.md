title: "Grow the heap on demand instead of reserving one fixed region"
status: ready
priority: P3
kind: feature
depends: []

# Grow the heap on demand instead of reserving one fixed region

DD#107 settled capacity with a large `NORESERVE` reservation (256 MB
default, `--heap-size` to change it). That removes the ceiling as a
practical problem but does not remove the ceiling: the heap is still one
contiguous region, and a program that outgrows it aborts.

The principled fix is to mmap another region when `sash_malloc` returns
NULL and add it to the allocator's free list, so programs are limited by
the machine rather than by a compile-time number.

## What this actually costs

**Correct the record from the parent task**, which claimed heap_4
"supports multiple regions via `vPortDefineHeapRegions`-style init".
**The vendored copy does not.** `sash-rt/csrc/heap4.c` has a single
`static BlockLink_t xStart` / `static BlockLink_t *pxEnd` sentinel pair
and an `xInitialized` guard, and `sash_heap_init(void *heap, size_t)`
takes exactly one region. Upstream FreeRTOS heap_5 is the multi-region
variant; heap_4 is not. So this is either a port to heap_5, or adding
region-chaining to the vendored heap_4 — real surgery on the allocator,
not a configuration change.

The clean-abort path (`sash_panic_oom`) stays regardless: a machine can
still run out of memory, and that must remain a diagnostic rather than a
crash.

## Gates

- A program that outgrows the initial region continues into a newly
  mapped one rather than aborting.
- Genuine OOM (the kernel refuses the mapping) is still a clean
  `sash: out of memory`, never a segfault.
- `--heap-size` keeps meaning something sensible — presumably the
  *initial* reservation.
- `hp.1`-`hp.3` need rework: they pin abort-on-exhaustion via an explicit
  1 MB heap, which on-demand growth would no longer exhaust.
