title: DD#34 — Minimize unsafe in sash-rt
status: implemented

1. Minimize runtime unsafe code: confine it to raw-pointer, allocation, startup
   and syscall boundaries, converting to safe representations immediately.

---

Thin unsafe boundary at each `extern "C"` entry point. Convert raw pointers
to slices immediately, then call safe Rust. ~80% of runtime is safe Rust.
Unsafe limited to: `_start`, allocator, syscalls, extern C signatures.
