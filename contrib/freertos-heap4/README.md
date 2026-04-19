# FreeRTOS heap_4

Source: [FreeRTOS-Kernel](https://github.com/FreeRTOS/FreeRTOS-Kernel)  
Author: Amazon.com, Inc. (originally Richard Barry)  
File: `heap_4.c`

## License

MIT — see `LICENSE` below.

The file here is the original, unmodified source from FreeRTOS-Kernel's
`portable/MemMang/heap_4.c`. The copy in `sash-rt/csrc/heap4.c` is a
clean-room adaptation that preserves the core algorithm (first-fit with
adjacent block coalescing) but removes FreeRTOS API dependencies.

## Usage in safebash

Provides `malloc`/`free`/`realloc` with coalescing for the sash-rt
runtime. Manages an mmap'd anonymous region initialized at `_start`.
Used by Pike's regexp (regcomp/regexec) and sash_table for dynamic
allocation.

## What it does

- First-fit allocation with block splitting
- Adjacent free block coalescing on `free()`
- No external dependencies — pure C, no threads, no locking
- ~170 lines in the adapted version
