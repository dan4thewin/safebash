title: "Stage echo/write operands and emit one complete write request"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "each historical argument-failure program below emits none of the builtin's own bytes, to stdout or to its redirect target, and retains its failure message/status in both backends"
  - "each successful fd-backed echo/write output is submitted as one complete write request, not separate argument/component/separator/newline writes, in standalone, redirected and child/stage execution"
  - "arguments retain DD86's left-to-right values before attached redirects; later mutation or reused formatting storage cannot change earlier prepared bytes"
  - "interior NUL bytes, real empty arguments, optional-argument elision and echo/write newline differences retain their existing semantics"
  - "pipe/FIFO requests up to PIPE_BUF include every byte in the non-interleaving bound; larger output is not rejected merely for exceeding that bound"
  - "short writes, interruptions and failures account for actual progress and never silently report a truncated output as complete; staging allocation faults remain fatal"
  - "compiler-owned captures remain memory copies with unchanged eligibility and DD127 taint classification"
  - "focused native/C CLI specs prove bytes and syscall shape, with pre-change/reversion controls and the normal predicted gate"

## Accepted scope, 2026-11-09

DG chose **"yes to a single write"** for `echo` and `write` (DD55,
journal 605). This task now owns that rule together with the existing
partial-prefix bug: both require preparing output before emission.
R2 in `q-f2-residuals` is closed as a policy question, not implemented.
The task's existing slug, P2 priority and open bug status are retained.

An fd-backed invocation submits its entire output in one request. Include
separators and `echo`'s final newline; `write` adds no newline. Do not
silently fall back to per-fragment writes for many operands or a long
record. `PIPE_BUF` is an atomicity-guarantee bound, not a new output-size
cap. There is no global writer lock, launch-order guarantee or blanket
atomicity promise for other destinations.

DD86 already requires source-order argument values before attached
redirects. This decision does not reopen that order, move stage arguments
to the parent, or roll back effects performed by argument expressions.
All output routes must preserve the same bytes and existing taint/use
checks; buffer captures do not become kernel-backed merely to satisfy a
syscall-count assertion.

## Historical argument-failure evidence

Measured on the base binary (1309385) and after F14, with the same result
in both backends:

| Program | stdout | abort message, status |
|---|---|---|
| `str x=""` / `echo "[${x:?m}]"` | `[` | `m`, 1 |
| `str x=""` / `echo a "${x:?m}" b` | `a ` | `m`, 1 |
| `int z=0` / `echo "[$(( 1 / z ))]"` | `[` | `division by zero`, 2 |
| `str x=""` / `write "[${x:?m}]"` | `[` | `m`, 1 |
| `str x=""` / `echo "[${x:?m}]" > out.txt` | (none; `out.txt` holds the one byte `[`) | `m`, 1 |

The command aborts while its arguments are being evaluated, but whatever
came before the failing part has already been written. An external echo
does not do this: `/bin/echo "[${x:?m}]"` prints nothing and aborts with
`m`, because every argument is evaluated before the exec.

These are the original F14 measurements, not fresh 2026-11-09 probes.
The original filing found no dedicated output-order contract. DD86's
subsequently accepted argument phases now settle the preparation order;
DD55 additionally requires a single output request. Neither is claimed
implemented by this recording.

## Cause

Both backends write a word one part at a time, lowering each part just
before its write: `echo_write_one_arg` (`sash/src/codegen/commands.rs`) and
`emit_echo_write_arg` (`sash/src/emit_c.rs`). A `:?` abort or an arithmetic
trap inside a later part, or in a later argument, therefore runs after the
earlier writes. `write` shares the path, through `rewrite_write_as_echo`
(`sash/src/transform/cmd_handlers.rs`).

## Implementation and evidence obligations

Prepare the complete output before submitting it. The rule does not
select a contiguous buffer, write vector, runtime ABI or lowering
representation. Choose a shared mechanism for both backends rather than
two independent formatter/length calculations. Respect actual syscall
size/vector limits without truncation or an ordinary fragmented fallback.

A part's storage must survive later operand evaluation. In particular,
retaining only a pointer into reused integer-formatting scratch storage
can print the last number twice; visiting a mutable variable earlier
without retaining its value has the same fault. Preserve explicit byte
lengths, including interior NULs, rather than routing output through
C-string argv conventions.

Prove single-request emission independently of output contents: one
passing concurrent run can merely have been scheduled without overlap.
Trace the compiled program, not the compiler or linker, and distinguish
the builtin's write from writes performed by argument helpers and the
harness. Cover multiple arguments, interpolation parts, separators and
newline, as well as `write`'s no-newline form and child/stage routing.
Keep byte assertions and bounded pipe/FIFO concurrency evidence too.
Memory captures need their own preservation control, not a kernel write.

Single-request publication is not failure atomicity. The current native
`echo_write_tracked` and C emitter test for a negative result, while
`sash_write` returns the actual byte count; that comparison alone cannot
prove full transfer. Include short-write and interruption paths in the
repair rather than reporting a dropped suffix as success. Coordinate
progress/cause handling with `runtime-failure-classification` under
DD55/DD130. No particular retry policy or new errno/status mapping was
chosen by the single-write ruling.

`q-capture-hoisting-order` owns general expression order;
`q-stage-capture-redirect-order` owns stage redirect timing. Reuse their
ordered values and preserve their invariants. Staging bytes alone does
not prove either repair, and their argument-order repairs alone do not
prove a single write.
`bug-write-diagnostics-name-echo` retains the source-command diagnostic
repair; shared output machinery must not lose the distinction between
the two public builtins.

Found during the batch-531 port (step F14, while re-measuring the base
reproducers that the DD#65 and DD#72 addenda cite). It is unrelated to the
then-active port and was filed under DG's backlog rule. The 2026-11-09
scope extension is policy/backlog recording only: no new behavior probes,
test-count predictions or compiler/runtime implementation accompany it.
