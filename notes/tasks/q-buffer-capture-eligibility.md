---
title: Consider wider compiler-owned capture buffering
status: backlog
priority: P3
depends: []
---

DG chose existing buffer paths first for DD127's read boundary (2026-11-02,
journal 560). Kernel reads introduce taint; a compiler-owned buffer copy
does not. Wider buffering is an optimization question, not permission to
trust a pipe or pathname.

Today any pipeline or redirect, a direct external, or a recursive purity
visit disqualifies the enclosing capture. Nested external capture followed
by attest and echo already permits a clean outer buffer (`outflow.11`).

Consider whether streaming wrappers and stdout-disconnected operations can
safely retain an outer buffer:

```
fn list {
  /bin/echo item | while readline x {
    attest x [[ -n $x ]]
    echo "$x"
  }
}
str result=$(list)
echo "${result:!}"
```

That result is currently kernel-backed. Any extension must preserve output,
status, nested capture frames and redirect scope, and make the actual runtime
buffer path agree with the shared eligibility predicate. Attesting before
a subsequent kernel read never attests that read. An external redirected
to `/dev/null` can reopen a saved parent memfd; pathname routing alone is
not an isolation argument.

Measure the benefit and discuss the eligibility change with DG before
implementation. Do not add dynamic taint tags, descriptor alias guards or
process isolation as an incidental part of this optimization.
