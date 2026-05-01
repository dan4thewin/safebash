# Design Decision #25: Arithmetic overflow traps and call depth limit

**Date:** 2025-04-16

### Arithmetic overflow

Bash integers are signed 64-bit with silent wraparound on overflow.
Safebash traps on signed overflow — it's almost certainly a bug, never
intentional in shell scripts.

**Implementation:** Cranelift provides `sadd_overflow`, `ssub_overflow`,
`smul_overflow` — instructions that return (result, overflow_flag).
The codegen emits a conditional exit on overflow. Three extra instructions
per arithmetic operation.

```
result, overflow = sadd_overflow(a, b)
if overflow → sash_exit(error)
```

No unsigned type. Bash doesn't have one. Bitmask operations on small values
(`logind_flags=$((flags | 2))`) never overflow in practice.

### Call depth limit

Tail self-recursion compiles to a loop (zero stack growth). Non-tail
recursion is a compile error. But mutual recursion and deep non-recursive
call chains could still overflow the stack — and in safebash's `#![no_std]`
runtime, stack overflow is undefined behavior (segfault or silent corruption).

**Solution:** Runtime depth counter. A global `call_depth` variable,
incremented on function entry, decremented on exit. If it exceeds a
compile-time limit (default 256 or 1024), the program exits cleanly.

**Considered and rejected: guard page.** Hardware-based stack overflow
detection via `mprotect` + signal handler on alternate stack. Zero per-call
overhead but: requires signal handler complexity in `#![no_std]` binary,
fires at the edge with no headroom for cleanup/diagnostics, platform-specific
guard page sizes, doesn't trigger on large stack frame jumps.

**Why depth counter wins for safebash:**
- Simple: five instructions per function call
- Deterministic: documented limit, consistent behavior
- Early detection: fires with plenty of stack left for error message
- Tunable: `safebash compile --max-depth=512`
- No stack-allocated buffers in safebash — strings are (ptr, len) on heap

**Guarantee:** If a safebash program compiles, it cannot:
- Overflow signed arithmetic silently
- Stack overflow (tail recursion → loop, everything else → depth counter)
- Non-tail recursion is a compile error

