title: DD#26 — FreeRTOS heap_4 Allocator
status: implemented

1. Use a first-fit free-list allocator with adjacent-block coalescing,
   supporting allocation, free and reallocation.

---

First-fit free-list allocator with block coalescing, adapted from
FreeRTOS heap_4. Supports malloc, free, and realloc. Compiled from C,
linked into sash-rt. Efficient for the append-heavy workload of shell
scripts with low fragmentation due to coalescing.
