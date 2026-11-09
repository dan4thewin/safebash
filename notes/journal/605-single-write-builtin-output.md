title: Single-write builtin output
date: 2026-11-09

## DG's decision

After the provisional scratch/regex recording, DG asked for the next
question. R2 in `q-f2-residuals` asks about concurrent builtin `echo`
children interleaving within a line. The AI recommended one write
request per invocation, including separators and newline, and the same
rule for `write` without adding a newline.

The proposal explicitly limited the guarantee to pipe/FIFO requests
whose complete size fits `PIPE_BUF`. Larger output may interleave;
other destinations have no blanket guarantee. Invocation order remains
unspecified, without a global lock. Buffering or a vector of fragments
costs staging storage but reduces syscall count.

DG answered:

> yes to a single write

DD55 records the accepted policy. This is recording/backlog work, not
authorization to implement the compiler/runtime change. DG selected the
rule; the AI reconciled its boundaries and implementation ownership.

## The contract

One fd-backed `echo` or `write` invocation presents its complete output
as one request. Do not split by argument, interpolation component,
separator or newline. `echo` includes its final newline; `write` does
not add one. Apply the rule to standalone, redirected and child/stage
execution, not only to background jobs.

DD86's existing phase order still applies: evaluate and retain arguments
left to right, then apply attached redirects, then emit the command's
output. A later argument failure cannot leave this builtin's own prefix
at the destination. A later mutation or reused formatting buffer cannot
alter earlier retained bytes. This does not undo independent effects
performed by argument expressions.

Preserve interior NUL bytes, real empty arguments, elision and the
existing formatting rules. Compiler-owned captures stay memory copies;
forcing a kernel write would change both routing and DD127 taint.
No new source flags, syscall ABI, staging representation or cache is
selected.

`PIPE_BUF` is the bound for a non-interleaving guarantee, not an output
length cap. One syscall is not all-or-nothing publication: an error or
short write can leave a prefix. Real progress and failures must remain
visible; silently dropping a suffix is not successful completion.
DD55/DD130 still govern legal handling, and staging allocation faults
remain fatal. Retry mechanics and errno/status mappings are not chosen
by this decision. The reference is
[POSIX write](https://pubs.opengroup.org/onlinepubs/9799919799/functions/write.html).

## Current source evidence

The clean recording baseline is `2e902f97`. The source index was
freshened through plain `sqlite3`; these are source observations, not
new execution results:

- Native `echo_write_separator`, `echo_write_one_arg` and `lower_echo`
  issue separate calls for separators, word components and newline
  (`sash/src/codegen/commands.rs:61-144,208-251`).
- C does the same in `emit_echo_arg`, `emit_echo_write_arg` and
  `emit_echo` (`sash/src/emit_c.rs:2628-2716`).
- `sash_write` first honors compiler-buffer capture routing; otherwise
  it calls the kernel writer and returns its byte count
  (`sash-rt/src/lib.rs:274-292`). The emitters' negative-result checks
  are not proof that the full requested byte count was transferred.
- `rewrite_write_as_echo` shares the output path by adding the internal
  no-newline flag (`sash/src/transform/cmd_handlers.rs:606-614`).
  This does not legalize source `echo -n`.

The old b17a concurrency result remains journal 539's measurement.
The existing argument-abort examples remain their F14 measurements.
Neither was rerun here.

## Ownership and proof

Extend the existing P2 `bug-echo-partial-line-before-abort`, retaining
its slug and open bug status, rather than create a second owner over
the same operand-preparation and fragment-emission path. This is the
AI's reversible task-organization choice, not a new language decision.
R2 is answered as policy; the residual-question file remains open.

The implementation must prove the two claims separately. Preparing
arguments first does not prove a single write, while assembling one
request from already-hoisted captures does not prove source-order
values. Coordinate with `q-capture-hoisting-order` and
`q-stage-capture-redirect-order`; retain the separate source-command
diagnostic owner, `bug-write-diagnostics-name-echo`.

Require syscall-shape evidence from the compiled program alongside
byte assertions. A lucky concurrent run is not proof: the fragmented
baseline can happen to run without interleaving. Include bounded
pipe/FIFO concurrency, large output, partial/error progress, preserved
memory captures, and before/reversion controls in both backends.
Exact spec IDs and gate counts belong to implementation, not this
recording.

DD55 is the authority; DD86/DD130, GUIDE, the affected task records,
findings index and status are coordinated. Compiler/runtime/converter,
spec and build sources are unchanged. No implementation branch,
behavior probe, spec sweep or fresh coverage measurement accompanies
this documentation.
