# Planned END-only explicit handling

DG expects END semantics to change in two ways: no source function
definitions inside END, and every fallible operation explicitly caught or
suppressed, with neither automatic abort nor automatic suppression.
DG then clarified: nothing outside END changes.

[DD#5](../decisions/DD05-planned-end-explicit-handling.md) records that
planned contract for the next C++ lifecycle slice. Current native DD#56
still specifies automatic diagnose-and-continue cleanup; no compiler or
runtime implementation was changed here.

The AI's automatic per-step cleanup-guard proposal is withdrawn. END will
show explicit handling at its own call sites, not an implicit cleanup mode.
Ordinary function bodies and DD#55's result-only `- f` keep their rules,
even when a function is called from END. Definitions remain outside END.

The roadmap, status, and contributor instructions now name the END-only
scope. Main-process ownership, at-most-once END, triggering-status
preservation unless explicitly overridden, and non-END signal/failure
rules are unchanged.
