---
title: "Automate the Darwin adapter call-fault controls"
status: open
kind: tooling
depends: []
gates:
  - "repeatable unprivileged Darwin controls exercise pipe/capture close-on-exec and unlink failures"
  - "operation, errno, unstarted stages, assignment/stdout preservation, and fd/child cleanup are asserted"
  - "inactive overrides pass and missing-injection or skipped-unlink/CLOEXEC mutations fail for their intended reason"
---

[Journal 008](../journal/008-macos-posix-runtime.md) records successful
real-Mac controls built from isolated adapter objects. The ordinary suite
is portable, but the test-only call bridge and its runner are currently
session evidence rather than tracked tooling.

Turn those controls into a small repeatable Darwin gate using the existing
runtime fixture/helper responsibilities. Override only adapter calls in
the test build, never production behavior or the pre-exec child.
Exercise reader and writer flag failures separately, capture flag failure,
and unlink failure. Keep target/stdout and descriptor snapshots pinned;
close partial pipes before checking for unreaped children.

Retain negative controls for skipped unlink and skipped pipe CLOEXEC.
Confine deliberately retained empty files to an owned test directory and
remove them individually. Use finite helpers and outer timeouts.

Do not require sudo/DTrace or describe function-call substitution as
kernel syscall injection. Keep the Linux strace catalog/subreaper separate,
and leave ordinary C++ tests hook-free. This task does not add lifecycle,
signal policy, an IR consumer, or a hosted CI route.
