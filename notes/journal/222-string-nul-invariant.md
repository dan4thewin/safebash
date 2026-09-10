# 222 — Runtime NUL-termination invariant (DD#87)

Found while testing DD#86's `str`: interpolating an int var and a string var
into one string and passing it to an **external** command produced trailing
garbage.

```
int n=5
s=hi
/bin/echo "n=$n s=$s"     # → "n=5 s=hi␦␦␦␦"
echo "n=$n s=$s"          # builtin → "n=5 s=hi"  (clean)
```

## Root cause

sash-rt is uniformly length-based (`(ptr, len)`), so the builtin `echo` (prints
`len` bytes) and all internal use were correct. But `execve` reads argv entries
as **C strings** (until a NUL). The string builders — `sash_concat`,
`sash_itoa`, `sash_substring`, `sash_join_args`, `sash_read_line/_delim/_all`,
`sash_epochseconds`, `sash_capture_end`, `sash_glob{,_regex}`,
`sash_regex_replace`, `expand_replacement`, table keys, `sash_itod` — allocated
exactly `len` bytes with **no terminator**. A computed external argument was
read past its end into allocator leftovers; whether it looked clean was luck
(that's why `"$s$n"` happened to print `hi5` and `"n=$n s=$s"` didn't). The spec
suite never caught it because specs use the **builtin** `echo`.

## Fix — DG chose approach B (whole-runtime invariant)

Every sash string is NUL-terminated at `[len]` by construction, so it is a valid
C string everywhere (argv can pass pointers directly). A new sash-rt helper:

```rust
unsafe fn alloc_cstr(len: usize) -> *mut u8 {
    let buf = sash_alloc(len + 1);
    *buf.add(len) = 0;
    buf
}
```

All string producers now allocate via `alloc_cstr` (or copy into one). Special
cases:
- **`sash_substring`** returned a *slice* into the subject (terminated only when
  it reached the subject's end) → now **copies** into an `alloc_cstr` buffer. A
  copy per substring, for correctness.
- **`sash_itod`** reuses a static buffer → re-writes `buf[len]=0` each call
  (guarded against a full 32-byte buffer).
- **`sash_regex_replace`** grows its buffer → NUL-terminates the final result at
  `[result_len]` (guarding the tight `len==cap` case).
- Strings that already inherit a NUL from their source keep passing pointers:
  environ (`sash_getenv`), argv (`sash_argv_get`), getcwd. Non-string
  allocations (tables, index/pointer/match arrays) keep plain `sash_alloc`.

## Verification

`spec/extern_args.sh` xa.1–8 (native,c) — computed args (concat, itoa,
substring middle/empty, join, mixed order) through `/bin/echo`, exact-match. The
class was invisible before because specs used the builtin echo; these
deliberately use the external one.

Full 3-mode sweep vs HEAD `9a145af`: **0 regressions, +16 passing** (xa.1–8 ×
native+c), 0 new failing. sash-rt-tests 32/32. The `+1` byte and the one
substring copy are the only runtime cost; internal length-based ops are
unchanged. DD#87. (Found incidentally during DD#86; that work is paused,
stashed, and resumes next.)
