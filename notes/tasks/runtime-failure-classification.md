title: Apply recoverable/fatal runtime failure classification
status: open
priority: P2
kind: work
depends: []
gates:
  - "an operation/cause/post-failure-state inventory applies DD130 to sash-owned runtime failures, distinguishing settled cases from any genuine remaining policy choices"
  - "runtime, native and C lowering, and ERR effect analysis use one coherent classification rather than independent setup-versus-command rules"
  - "fork, runtime heap and arithmetic faults keep their fatal contract; ordinary path-specific file errors remain recoverable"
  - "resource errors from open/pipe/dup and restoration failures are classified explicitly; an unknown or unhandled cause never becomes successful continuation"
  - "ERR requires handling of recoverable failures where they occur; the possibility of a classified fatal fault alone does not ban the operation"
  - "focused native/C CLI cases and independent negative/reversion controls pin both classes, followed by the normal predicted gate"

## Decision and scope

DD130 (DG, 2026-11-06; journal 581) replaces setup position as the reason
for allowing or refusing recovery. This task owns the shared classification
and the runtime/compiler interface that preserves a failure's cause until
the handling decision is made.

Inventory sash-owned syscall and runtime error paths, including process
creation, file open/read/write, exec, pipes, descriptor backup/dup/restore,
captures, directory enumeration and existing fatal arithmetic/allocation
paths. Use actual producer and cleanup paths, not merely a census of
diagnostic text.
Retain an explicit disposition for each affected family; do not silently
leave an ordinary file failure fatal because its current API cannot
return an error.

DD131/DD132's provisional selectors/immutable-scratch/regex branch trial
(`q-glob-semantics`) needs the affected operation/cause/publication
classification. In particular, EOF/no matches are not read/open errors,
and partially produced entries must not escape on failure. Account for
allocation faults and actual globals visible to ERR without weakening
their fatal class. Ordinary targets retain their old data on operation
error; scratch attempts remain fresh and empty. Invalid regex candidates
give quiet false at `r?`, not a fatal fault or a directory/no-match error.
Regex validity does not erase possible I/O failures. The trial is a consumer
of this shared policy, not a reason to create another per-backend classifier.

The mapping may depend on the operation, errno and known post-failure
state. In particular, do not classify every open failure as path-local,
every resource-looking errno globally the same way, or every
backup/restore failure alike. Apply the accepted criterion and existing
DDs; batch any genuinely consequential, underdetermined cases for DG.

Reuse existing shared mechanisms and fatal dispatch. Do not prescribe a
new ABI, private per-backend classifier or duplicated walker merely to
implement the distinction. The syscall-layer migration (DD129) is separate:
this task changes error semantics, not which binding issues the syscall.

## Dependent work and preserved boundaries

`recoverable-file-failures` consumes this classification to implement
usable file-error handling. The existing fork task owns by-form fatal
routing and can use the existing fatal primitive without waiting for
unrelated I/O classifications.

Keep DD128's once-only/main-process handler ownership, allocation-free
dispatch and actual-global state, DD55's no-reach rule, and typed function
results. A child's completion status is not a new process-wide fatal
notification protocol. Do not infer a failure class from its numeric status.

The main implementation surfaces are `sash-rt/src/`, native codegen,
`emit_c.rs` and `terminal.rs`. ERR's compile-time model must describe the
runtime paths that actually ship; neither blanket SB5101 exemptions nor
unimplemented handling promises satisfy this task.
