---
title: "Repairing the capture-report failure probe"
date: 2026-10-31
---

DG proposed capturing the output, saving status in the `||` handler,
attesting once unconditionally, and only then branching. DG subsequently
asked the AI to repair the unfinished capture-report example.

The AI applied that spelling to
[capture_report.sash](../../sash-cpp/examples/capture_report.sash):

```sash
int probe_status=0
probe=$(/bin/false | /bin/cat) || probe_status=$(status)
attest probe [[ $probe == '' ]]
if (( probe_status == 0 )) { ... } else { ... }
```

The status is saved before the attest can consume it. The entire `||`
statement precedes one unconditional attest, eliminating the previous
unread-capture, tainted-read, and else-adjacency diagnostics without changing
the validator. The later branch preserves the existing failure message.
The local C++ draft likewise checks the captured output before inspecting
pipeline success, rather than placing its guard only in `else`.

The native branch-eligibility question stays in
[its native task](../../notes/tasks/q-capture-status-attest-sequence.md);
that question does not block this already-supported spelling.

## Checks and publication boundary

The complete repaired native program matches the independent eight-line
expected report. All five native goldens pass with DG's supplied compiler
and runtime. The local C++ worktree passes GCC's twenty-three fixture
groups, five C++ goldens, and formatting.

Publish the independent sash source, expected output, native golden entry,
and repair records. The native golden entry is staged separately from the
existing C++ build/test additions in `mkfile`, so the published C++ build
still contains four pairs.

The C++ companion, capture API implementation, helper addition, and C++
build integration remain local drafts. Their focused capture fixtures,
fault catalog additions, and full compiler/sanitizer milestone gates are
not complete. No new IR reader, producer change, or native dependency is
introduced. The existing runtime task remains open.
