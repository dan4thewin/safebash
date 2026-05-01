title: DD#12 — Null-Terminated Counted Strings
status: implemented

Strings carry both a pointer and a length, but are also null-terminated
for C interop. Best of both worlds — O(1) length queries and compatibility
with POSIX APIs.
