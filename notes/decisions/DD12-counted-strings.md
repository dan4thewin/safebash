title: DD#12 — Null-Terminated Counted Strings
status: implemented

1. Strings carry a pointer and byte length and have a trailing NUL for C
   interoperation; operations use the counted length, including interior NULs
   (DD87). The trailing terminator is not a content byte.

---

Strings carry both a pointer and a length, but are also null-terminated
for C interop. Best of both worlds — O(1) length queries and compatibility
with POSIX APIs.

Interior NUL still requires boundary-specific treatment. DD99 rejects it
in external argv; DD135 requires filename predicates to return false before
lookup and defines `0?` over the counted contents, not the terminator.
The predicate and filename guards are implemented in both backends
([journal 639](../journal/639-counted-nul-and-filename-guards.md)).
