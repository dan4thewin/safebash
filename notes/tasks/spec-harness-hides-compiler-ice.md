title: "spec harness: a compiler ICE reaches specs with an empty $err (rc 1 native, 101 C)"
status: open
priority: P2
kind: infra
depends: []
gates:
  - "an ICE in either backend, at any phase, is visible to every spec: its text lands in `$err`, or the harness fails the check outright"
  - "the same decision is made, explicitly, for a compiler hang (rc 124), which today passes a check that asserts only `$rc != 0 && -z $out`"
  - "a sweep with the change shows which passing specs, if any, were masking an ICE; each one is a finding"
  - "a reversion proof: a stubbed compiler that ICEs in native codegen turns a spec red that asserts only `$rc != 0 && -z $out`"

## What

`spec/common.sh` keeps only some stderr lines in `$err` (lines 173 and 213):

```sh
err=$(grep -E '^\S+:[0-9]+:[0-9]+:|^sash:|: (ICE|tainted)' err.raw | grep -v 'static binary' || true)
```

The native driver catches a codegen panic and prints it as
`FILE:ICE: msg` with no space (`compile_native`, `sash/src/lib.rs`:
`eprintln!("{}:{}", input, …)` over `compile_to_object`'s `Err`), then exits
**1**. None of the three alternatives matches that line, and the panic hook's
own lines (`thread 'main' … panicked at …`, `ICE: …`) do not match either. So
a native codegen ICE reaches a spec as an ordinary compile failure, rc 1, with
an empty `$err`.

In C mode the same ICE is an uncaught panic, rc **101**, and its `ICE: …` line
is filtered out too, so `$err` is empty there as well. Only the exit code
tells the two apart.

## Measured

Found during the batch-531 F13 port (journal 532), by the reversion stub that
removes the stage route's `tail_loop.take()`. The program
`fn f { return g $1 }; fn g { if (( $1 > 0 )) { f $(( $1 - 1 )) | cat }; echo "g$1" }; g 2`
then ICEs in both backends:

- native: rc 1, stderr `p.sh:ICE: undefined array: ` (panic at
  `codegen/types.rs:285`); the harness sees `rc=1`, `err=`;
- C: rc 101, panic at `emit_c.rs:4449`
  (`collection `` has no emitted SSA slot`); the harness sees `rc=101`, `err=`.

The spec for that program (`stgcyc.1`) asserts its code, so it went red
either way.

2026-10-28 (F2, journal 539): the ICE above was B120's, not the missing
take's. The stage took the caller's positional array before it lowered its
own arguments, and `$(( $1 - 1 ))` reads the caller's `$1`. F2 fixed B120,
so this program no longer ICEs, stub or not; the harness gap is unchanged,
and a new reproducer needs some other ICE. A spec that asserts only `$rc != 0 && -z $out` would have
passed on the ICE.

Confirmed through the harness itself: sourcing `spec/common.sh` and calling
`run_sash` with `SASH` pointed at a script that replays the stub's stderr
and exit code gives `rc=1 err=[]` (native) and `rc=101 err=[]` (C), and the
check above passes in both modes.

A compiler **hang** has the same shape. `_run_sash_native` sets `rc=124`,
`out=""` and an `$err` that names the hang guard, and nothing fails the
check, so `$rc != 0 && -z $out` passes on a hang too (measured the same way,
with `SPEC_TIMEOUT=1` and a compiler that sleeps). The hang guard's comment
says the spec "must FAIL loudly"; it does only for a check that reads `$err`
or `$rc`'s value.

## Not measured

How many specs assert a compile failure without asserting the diagnostic's
content. AGENTS.md already says a bare `-n $err` is not an assertion; this
is the converse: a check that never reads `$err` cannot see an ICE at all.

## Options (to decide when the task is taken)

- widen the filter to `:ICE:` and the panic hook's `ICE:` line, so the text
  lands in `$err`;
- or have the harness fail the check outright on an ICE (and on a hang),
  whatever the check asserts. Nothing in `chk` does this today: it compares
  the last command's status and nothing else.
