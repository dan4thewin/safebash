title: DD#48 — Out-of-range positional detection
status: implemented

At function call sites, scan body IR for highest `$N` reference.
If call provides fewer arguments than function reads, emit compile-time
error. Tracks cumulative shift count.
