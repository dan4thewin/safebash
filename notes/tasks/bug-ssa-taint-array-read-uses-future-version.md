title: "SSA array reads use a future version's taint"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "The reproducer compiles in native and C modes and prints clean then dirty"
  - "sflow.19 replaces sflow.5 with an acceptance assertion over the same source"
  - "Earlier array reads cannot be cleaned by a later attest or overwrite in the backstop"

Fixed for ordinary collection operands in journal 568. Cell reads, splats,
table iteration and map-fill sources retain their reaching `VarRef` instead
of asking for the highest version of a bare name. Both backends still use
the same physical table storage.

`sflow.19` requires the source below to print `clean` then `dirty`.
`adjssa.11`-`.14` independently pin earlier reads and derived values against
later attests. All four fail on the intermediate fixed-point compiler
before versioned collection operands; the final compiler rejects them.

This does not claim to repair the bare-name lookup used by unlowered
redirect operands, or to replace the completed parent view inherited by
children with a precise fork-time snapshot. Those are separate remaining
backstop boundaries, recorded in DD127.

Found during W1b-2, 2026-11-02. The slot-summary validator accepted the
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

Both backends rejected. The retired `sflow.5` pinned that intermediate boundary, not a
claim that the program is accepted.

Cause: `ssa_taint.rs::check_process` classified every definition in its
process view before checking any use. A bare-name collection read calls
`taint_of_name`, which chooses the highest numbered version in that
completed view, not the version reaching that read.

W4 (journal 563) isolated processes but retained this lookup and passed the
completed parent view to each child, including definitions after the fork.
It therefore did not fix this timing defect. Per DG's port-scope rule it
remained separate until definition convergence made it directly coupled.

Simply checking while iterating block IDs would not establish execution
order. The fix retains the backstop's independent checks and process
boundaries; it does not silence the array-read check.

The reverse weakness was also visible under W1b-2's flow reversion:
`sflow.7` compiled and printed its unattested input before filling and
attesting the same collection. The deliberately broken validator had
dropped the caller's taint, and the backstop's later clean version masked
the earlier read. The restored validator rejects this program with
call-site SB5020; this is backstop evidence, not a shipped validator bypass.

Evidence: the slot unit's `before-collparam-*` / `after-collparam-*`
records in session `files/port-w1b2-slots-probes.tar.gz`; journal 559.
