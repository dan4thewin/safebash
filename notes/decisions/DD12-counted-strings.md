title: DD#12 — Null-Terminated Counted Strings
status: implemented

1. Strings carry a pointer and byte length and have a trailing NUL for C
   interoperation; operations use the counted length, including interior NULs
   (DD87).

---

Strings carry both a pointer and a length, but are also null-terminated
for C interop. Best of both worlds — O(1) length queries and compatibility
with POSIX APIs.
