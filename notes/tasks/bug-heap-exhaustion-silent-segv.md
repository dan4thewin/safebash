---
status: done
priority: P1
kind: bug
depends: []
---

# Heap exhaustion is a silent SIGSEGV: the heap is a fixed 1 MB and
# allocation failure is never checked

## The defect

`sash-rt/src/lib.rs:943`:

```rust
const HEAP_SIZE: usize = 1024 * 1024; // 1MB, no BSS cost
```

`sash_init_heap` mmaps exactly that much and hands it to heap_4 (DD#26).
heap_4 returns `NULL` when it cannot satisfy a request
(`sash-rt/csrc/heap4.c:67,71,154`). **No caller checks.**
`alloc_cstr` (lib.rs:1002) immediately writes the NUL terminator:

```rust
unsafe fn alloc_cstr(len: usize) -> *mut u8 {
    let buf = sash_alloc(len + 1);
    *buf.add(len) = 0;      // ← NULL deref when the heap is full
    buf
}
```

So a program that allocates past 1 MB dies with SIGSEGV, no message, no
exit status a script can test, and no indication of which line did it.

This is the class this batch keeps producing, in its worst form: a call
that cannot satisfy its contract returns a plausible-looking value
(`NULL` is a valid `*mut u8`) instead of failing loudly.

## Measured, on the live binary (2026-09-12)

Reachable from ordinary input, not a stress case.

```sh
# t2.sh — DD#99 read into an array
array a=()
read a
echo $(len a)
```

| stdin                          | bytes  | result             |
|--------------------------------|-------:|--------------------|
| 4000 × `line%06d\0`            | 44 000 | `4000`             |
| 4400 × `line%06d\0`            | 48 400 | **SIGSEGV (139)**  |
| 5500 × `a\0`                   | 11 000 | `5500`             |
| 6000 × `a\0`                   | 12 000 | **SIGSEGV (139)**  |

Not specific to `read` — a plain array build does it too:

```sh
array a=()
int i=0
while (( i < 30000 )) {
  a[$i]="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  i=$(( i + 1 ))
}
echo $(len a)          # → SIGSEGV
```

The thresholds are far below 1 MB of payload (12 000 bytes of data kills
it), which points at the second defect below.

## Second defect: the table leaks every generation on growth

`sash_table_set` (lib.rs:1550–1557) doubles the entry array by allocating
a new one and copying — and **never frees the old one**:

```rust
let new_entries = sash_alloc((new_cap as usize) * size_of::<TableEntry>()) as *mut TableEntry;
core::ptr::copy_nonoverlapping((*t).entries, new_entries, (*t).entry_count as usize);
(*t).entries = new_entries;          // old buffer leaked
```

`table_grow` reallocates `indices` the same way — check it too. Growth is
geometric so the leak is bounded at roughly 1× the live size, but against
a 1 MB ceiling that halves the usable heap, and heap_4's coalescing cannot
reclaim blocks nothing freed. This alone explains why 12 000 bytes of
payload exhausts a 1 MB heap.

## Third defect: `sash_read_all` truncates at 64 KiB and eats errors

`sash-rt/src/lib.rs:3621`:

```rust
pub extern "C" fn sash_read_all(fd: i64) -> (*mut u8, usize) {
    let mut buf = [0u8; 65536];
    loop {
        if total >= buf.len() { break; }     // silent truncation
        ...
        Err(_) => break,                      // silent read error
    }
```

A file or stream over 64 KiB is **silently truncated** — the caller gets a
short value with no indication. A read error is indistinguishable from
EOF. This is on the DD#99 `read <fd> ARRAY` path and would be on the
`$(<file)` path too if `dollar-lt-file-forks` is built on it (see that
task — this must be fixed first or that fix inherits the bug).

Note also the 64 KiB **stack** buffer in a `#![no_std]` runtime.

## Options for DG — runtime memory architecture

The bug must be fixed; *how* is the fork.

**A. Grow the heap on demand.** On `sash_malloc` returning NULL, mmap
another region and add it to heap_4's free list. Programs become limited
by the machine, as users expect of a compiled program.

> **Correction (2026-09-13).** This option originally claimed heap_4
> "supports multiple regions via `vPortDefineHeapRegions`-style init".
> **It does not.** The vendored `sash-rt/csrc/heap4.c` has a single
> `xStart`/`pxEnd` sentinel pair and a one-region
> `sash_heap_init(void*, size_t)`; heap_5 is the multi-region variant
> upstream. A is allocator surgery, not configuration. The estimate
> below was written against the false claim.
*Cost:* most work; heap_4 multi-region init needs care; the failure path
still needs option C for genuine OOM.

**B. Reserve a large region up front.** `mmap` e.g. 256 MB with
`MAP_NORESERVE`; pages are only committed when touched, so RSS still
tracks real use and there is no BSS cost. One-line change.
*Cost:* still a hard ceiling, just a distant one; picks an arbitrary
number; a 32-bit target would not tolerate it.

**C. Check for NULL and abort with a diagnostic.** Orthogonal to A/B and
**required regardless** — it converts a silent SIGSEGV into
`sash: out of memory` on stderr with a non-zero status.
*Cost:* touches every allocation site, or is centralised in `sash_alloc`
with an `abort_oom()` that never returns.

**Recommendation: C first (it is the actual "fail loudly" fix and is
independent), then B as the cheap capacity raise, with A filed as the
principled follow-up.** Also fix the table leak and the `sash_read_all`
truncation as part of the same work — they are what makes the 1 MB
ceiling bite at 12 KB of data.

## Gates

- A program that exceeds the heap prints a diagnostic and exits non-zero;
  it never raises SIGSEGV.
- `read a` on a 1 MB NUL-separated stream succeeds, or fails cleanly.
- A 30 000-element array build succeeds, or fails cleanly.
- `sash_read_all` on a >64 KiB stream returns all of it, or fails — it
  does not silently return a prefix.
- A read error is distinguishable from EOF.
- `sash_table_set` growth frees the previous generation.

## Found

While starting `dollar-lt-file-forks` (b5), which proposes building
`$(<file)` on `sash_read_all`. Reading that function to check it was
suitable exposed the 64 KiB truncation, and testing the existing DD#99
`read` path against a large stream produced the segfault.

## Partial fix landed 2026-09-12 — option C, plus the two contributing bugs

Option **C** is not a fork: "fail loudly when you cannot resolve or satisfy
something" is a stated project principle, and the bug policy makes fixing
mandatory. Implemented without waiting. The **capacity** question (A vs B)
is untouched and still needs DG.

- `sash_alloc` checks for NULL and calls a new `sash_panic_oom(n)`, which
  writes `sash: out of memory (requested N bytes)` to fd 2 and exits 1 —
  following the existing `sash_panic_argv` pattern.
- `sash_table_set` frees the previous entry array on doubling;
  `table_grow` frees the previous index array. Both were leaked every
  generation, which is why a 1 MB heap died at ~12 KB of payload.
- `sash_read_all` no longer reads into a fixed 64 KiB **stack** buffer. It
  grows a heap buffer, retries `EINTR`, and aborts via a new
  `sash_panic_read()` on a real error rather than reporting it as EOF. It
  also reserves the DD#87 NUL terminator explicitly.

### Measured after

| case                             | before            | after                        |
|----------------------------------|-------------------|------------------------------|
| 30 000-element array build       | SIGSEGV (139)     | `sash: out of memory`, rc 1  |
| `read a` on 20 000 × `a\0`       | SIGSEGV (139)     | `sash: out of memory`, rc 1  |
| `read a` on 6 000 × `a\0`        | SIGSEGV (139)     | `6000` — the leak fix paid   |
| 100 KB stream, one separator     | truncated to 64 K | both elements, 70000 + 30000 |

Specs `hp.1`–`hp.5` in `spec/misc.sh` (native,c; janet runs on the Janet VM,
not sash-rt). Sweep 5183/38/549, zero non-janet failures, zero flips.

## Closed 2026-09-13 — option B, as DD#107

DG chose **B**, 256 MB, with the size compiled in rather than read from
the environment and a flag to change it. See `DD107-heap-capacity` for the
reasoning; the short form is that safebash deliberately prevents the
environment from steering a running binary, so a `$SASH_HEAP` would have
been a way back in.

- `heap4.c` defines `sash_heap_size` **weakly** at 256 MB;
  `sash_init_heap` maps that much with `MAP_NORESERVE`.
- `sash --heap-size=<size>` (`K`/`M`/`G` suffixes) emits a **strong**
  definition of the same symbol, which the linker prefers. Both backends:
  a Cranelift data object natively, a C global under `--emit-c`.
- Invalid operands are rejected at compile time — junk, zero, and
  overflow — rather than clamped.

### Measured after

| case                              | 1 MB heap          | 256 MB default |
|-----------------------------------|--------------------|----------------|
| 5 000-element array build         | `5000`             | `5000`         |
| 10 000-element array build        | out of memory      | `10000`        |
| 200 000-element array build       | out of memory      | `200000`       |
| hello-world peak RSS              | 340 kB             | 340 kB         |

`NORESERVE` is what makes the last row hold: the reservation costs
address space, not memory.

Specs: `hp.1`-`hp.3` were rewritten to compile against an explicit
`--heap-size=1M` so that exhaustion is pinned to a *stated* ceiling —
otherwise raising the default would have silently stopped them testing
the failure path at all. `hp.6`/`hp.7` and `cli.13`/`cli.20`-`cli.23` are
new.

**A remains open** as `heap-multi-region-growth` (P3): the ceiling is
distant now, but it is still a ceiling.
