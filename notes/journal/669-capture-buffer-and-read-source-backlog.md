title: Capture buffering and read-source findings backlogged
date: 2026-11-24

# Capture buffering and read-source findings backlogged

DG questioned SB5020 at `_tmp/t1.sb:480`: the captured
`resolve_reboot_type` function contains no external commands. The AI
isolated the function with a clean literal map and argument, then separated
two causes. DG asked to file both in the backlog, not implement them.

## A negated test selects the kernel-backed capture

The outer `!` in `if ! [[ $entry =~ : ]]` becomes a negated one-command
pipeline in the AST. The shared buffer classifier deliberately rejects
every pipeline, including this representation of a single test. The
enclosing capture therefore uses a memfd, and DD127 makes its later kernel
read a new taint acquisition.

A minimal function containing only that test and `echo` reproduces SB5020
with the real native compiler at `54b49dd0`. Moving the `!` inside the
brackets makes that minimal control compile. Neither contains an external
command, string split or nested substitution.

The existing P3
[buffer-eligibility task](../tasks/q-buffer-capture-eligibility.md)
now owns this concrete case alongside its broader questions. The AI reused
that owner rather than creating a competing task. No eligibility change or
exception to kernel-read taint has been adopted.

## The read source has a separate lowering defect

Moving the negation inside the test does not make the complete resolver
compile. It exposes six local SSA-backstop diagnostics. Reducing the body
to `read ${1:s/,/$'\x00'/g} _a` followed by iteration leaves one of them;
plain `read $1 _a` and directly echoing the substituted value compile.

The suppressed substitution carrier is not materialized before `SplitValue`
when the read has no attached redirect scope. The backstop treats that
unlowered body as tainted; with taint disabled, native and C both hit the
backend guard saying `CmdSubstBody` should have been lifted. The ordinary
rejection is therefore masking a genuine lowering defect, not identifying
tainted input in this example.

New P2
[bug-read-source-capture-unlowered](../tasks/bug-read-source-capture-unlowered.md)
records the reproducer, actual native/C outcomes, source locations and
repair gates. It is independent of the buffer-eligibility policy and the
previously filed cell-write acquisition hardening.

## Boundaries

The AI neither modified nor executed the full operational script. Its
compile-only check stopped earlier on unavailable target-host commands;
the isolated resolver and reduced programs supply the diagnostic evidence.
The minimal controls were compiled only, and their temporary outputs were
removed.

The user-owned script changed during backlog recording. Those edits were
left untouched; the task records contain self-contained reproductions.

This change records deferred work on `err-attempt-trial`. Compiler,
runtime, specs and DDs are unchanged; DD136 remains provisional and main is
not advanced. No full spec sweep or fresh coverage measurement was run,
and the generated status block is retained unchanged.
