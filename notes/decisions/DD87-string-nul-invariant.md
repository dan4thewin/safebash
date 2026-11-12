title: "DD#87 — Runtime invariant: every sash string is NUL-terminated"
status: accepted

1. Every runtime string has `len` content bytes and a NUL terminator at
   `[len]`, while internal operations retain the explicit length.
2. String producers allocate room for the terminator; partial slices copy
   into terminated storage, and reused buffers rewrite the terminator.
   Existing compliant storage may be shared.
3. This invariant applies to strings, not binary allocations. It does not
   make an interior NUL safe at an external argv boundary (DD99).

---

Settled with DG (2026-09-01). Every **string** value produced by the sash
runtime (`sash-rt`) is NUL-terminated at `[len]`: a buffer of `len` content
bytes is allocated as `len + 1` bytes with `buf[len] = 0`.

## The bug that forced it

sash-rt is uniformly **length-based** — `(ptr, len)` pairs — so internal use and
the builtin `echo` (which prints `len` bytes) were always correct. But `execve`
consumes argv entries as **C strings**: it reads each `char*` until a NUL. The
string builders (`sash_concat`, `sash_itoa`, `sash_substring`, …) allocated
exactly `len` bytes and wrote no terminator, so a computed argument passed to an
**external** command was read past its end into whatever the allocator left
there:

```
int n=5
s=hi
/bin/echo "n=$n s=$s"     # → "n=5 s=hi␦␦␦␦"   (garbage; allocator luck)
echo "n=$n s=$s"          # builtin, length-based → "n=5 s=hi"  (always clean)
```

The spec suite never caught it because specs use the builtin `echo`. Silent
corruption of every external command argument built from a non-literal string.

## The decision — approach B (whole-runtime invariant)

Two fixes were possible: (A) NUL-terminate each argument at the exec boundary
(`build_argv_*`), or (B) make **every** runtime string builder NUL-terminate, so
a sash string is a valid C string everywhere by construction. DG chose **B** —
argv (and any future C-string boundary) can then pass pointers directly, and
there is one uniform rule instead of a boundary to remember.

## The rule

- A helper `alloc_cstr(len)` (sash-rt) allocates `len + 1` and writes `buf[len]
  = 0`; the caller fills `buf[0..len]`. **All string producers use it.**
- Producers that return a **slice** into another buffer (e.g. `sash_substring`)
  must instead **copy** into an `alloc_cstr` buffer — a partial slice is
  terminated only when it happens to reach the source's end. This trades a copy
  for correctness (accepted).
- Producers backed by a **reused static buffer** must re-write the terminator
  at `[len]` each call (a shorter value leaves a stale byte). The rule's
  original subject, `sash_itod`, was removed with DD#36; the invariant stands
  for any future static-buffer producer.
- Strings that already inherit a NUL from their source keep passing pointers:
  environ values (`sash_getenv`), argv entries (`sash_argv_get`), the
  getcwd/readlink syscall buffers. Table **values** store the caller's pointer
  (which is now itself invariant-compliant); table **keys** are copied via
  `alloc_cstr`.
- **Non-string** allocations — tables, index arrays, pointer arrays,
  regex-match arrays — keep using `sash_alloc` (no terminator; they are binary).

## Maintenance

Any new sash-rt function that returns a string value MUST produce it via
`alloc_cstr` (or copy into one). A `#[test]` / spec that passes a computed
string to an **external** command (`/bin/echo`, not the builtin) guards the
class — see `spec/extern_args.sh`. Journal 222.
