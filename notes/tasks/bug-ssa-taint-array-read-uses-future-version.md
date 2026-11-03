---
title: "SSA array reads use a future version's taint"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "The reproducer compiles in native and C modes and prints clean then dirty"
  - "sflow.5 becomes an acceptance assertion without changing its source"
  - "Earlier array reads cannot be cleaned by a later attest or overwrite in the backstop"
---

Found during W1b-2, 2026-11-02. The slot-summary validator accepts the
earlier clean call, but the SSA backstop rejects it:

```
fn show { echo "${A[0]}" }
array a=(clean)
show =a
argv 1 a
echo "${a[0]:!}"
```

Pass `dirty` as the program argument. At 685ea2f1 the validator reports
SB5020 inside `show`, because the old slot analysis uses the final
top-level taint set. With W1b-2 that error is gone, exposing:

```
tainted array a[...] used as an argument to 'echo' without attest (SSA taint backstop)
```

Both backends reject. `sflow.5` pins this intermediate boundary, not a
claim that the program is accepted.

Cause: `ssa_taint.rs::check_process` classifies every definition in its
process view before checking any use. A bare-name collection read calls
`taint_of_name`, which chooses the highest numbered version in that
completed view, not the version reaching that read.

W4 (journal 563) isolates processes but retains this lookup and passes the
completed parent view to each child, including definitions after the fork.
It therefore does not fix this timing defect. Per DG's port-scope rule,
this separate, unported fault remains backlog work.

A fix needs a per-use collection version or proper reaching-definition
state. Simply checking while iterating block IDs would not establish
execution order. Keep the backstop's independent checks and process
boundaries; do not silence the array-read check.

The reverse weakness was also visible under W1b-2's flow reversion:
`sflow.7` compiled and printed its unattested input before filling and
attesting the same collection. The deliberately broken validator had
dropped the caller's taint, and the backstop's later clean version masked
the earlier read. The restored validator rejects this program with
call-site SB5020; this is backstop evidence, not a shipped validator bypass.

Evidence: the slot unit's `before-collparam-*` / `after-collparam-*`
records in session `files/port-w1b2-slots-probes.tar.gz`; journal 559.
