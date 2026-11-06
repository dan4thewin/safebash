title: Write builtin stderr output without descriptor backup/restore
status: open
priority: P2
kind: bug
depends: []
gates:
  - "simple echo/write stdout-to-existing-fd duplication, including >&2 and 1>&2, writes directly to the destination without saving, replacing or restoring process descriptors"
  - "native and C backends share the destination semantics; ordinary code and handlers use the same mechanism"
  - "ERR accepts caught or suppressed stderr writes while still rejecting an uncaught write failure; SB5101 follows actual lowered effects rather than a blanket redirect exemption"
  - "stdout, stderr, newline behavior, captures, operand evaluation and the handler's saved status remain correct"
  - "other and compound redirects retain their ordered setup, side effects and failure obligations"
  - "focused native/C specs, a pre-change refusal control and evidence that descriptor backup/restore is absent precede the predicted ordinary gate"

## Finding

DG reported SB5101 when writing to stderr from ERR, then pointed out that
stderr is already open and should work like stdout. DG requested this fix
be added to the backlog on 2026-11-06; journal 578 records the discussion.

Measured against `649c19b2`:

```sh
ERR { - echo "error" >&2 }
fail 7
```

This rejects with SB5101 for an "uncatchable descriptor backup/restore
failure". Catching the echo with `if` rejects for the same reason.
The otherwise identical suppressed stdout echo compiles, prints `error`
to stdout and exits 7.

The desired stderr form prints `error` plus newline to stderr, nothing to
stdout, and preserves status 7. Recoverable write failures must remain
catchable/suppressible, not silently succeed; classified fatal faults
remain fatal (DD130).

## Cause and scope

Native `codegen/commands.rs::lower_echo` hard-codes fd 1, as does C emission.
The generic redirect wrapper consequently saves fd 1, duplicates fd 2 onto
it, runs the builtin, and restores fd 1. `terminal::redirect_failure`
correctly sees those uncatchable operations and refuses them in ERR.
The operations are unnecessary for this simple builtin output destination:
the write can use fd 2 directly without changing either descriptor.

Carry the actual output destination through lowering so the backends and
ERR failure analysis agree about which operations execute. Cover `write`
as well as `echo`; its shared echo lowering only changes newline behavior.
This is not a new stderr builtin, an ERR-only privilege, or permission to
ignore write errors. An unhandled echo in ERR must still be rejected under
DD128's explicit failure-handling requirement.

Do not generalize this into dropping arbitrary redirect lists. Preserve
ordered file-open side effects, descriptor mappings, diagnostic routing,
operand/capture behavior and the existing handling rules wherever real
setup or restoration remains. General file-error classification and
redirect catchability belong to `runtime-failure-classification` and
`recoverable-file-failures`. This direct-destination repair is independent:
remove the unnecessary operations, not just their SB5101 rejection.

## Evidence required

Use the real CLI and both backends. Pin stderr placement, unchanged stdout,
echo versus write termination, saved handler status and handled write
failure. Confirm the original compiler refuses the ERR spelling above.
Also prove the direct path performs no redirect save/dup/restore; use a
positive control that still exercises the generic redirect machinery.
Merely suppressing SB5101 while retaining those operations is not a fix.
