title: DD#48 — Out-of-range positional detection
status: implemented

1. Infer each function's argument requirements from its body and enforce them
   at every call site, under DD81/DD84's fixed/variadic rules.
2. A call cannot silently supply an empty value for a required missing argument.

---

At function call sites, scan body IR for highest `$N` reference.
If call provides fewer arguments than function reads, emit compile-time
error. ~~Tracks cumulative shift count.~~ DD75 retires `shift`.
