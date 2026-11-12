title: "SB5025 renders placeholder `[N]` as code, inviting a literal reading"
status: done
completed: 2026-09-28
priority: P4
kind: bug
depends: []
gates:
  - "no diagnostic renders metavariable placeholders inside backticks where they could be mistaken for literal syntax"
  - "`argv [2] a` (literal brackets) is rejected at compile time rather than ICEing at runtime"

## Symptom

SB5025 advises:

> `$(argv)` is an array splat — it is valid only as the sole source of a
> for-in loop; snapshot into an array with `argv [N] a`, or use `$(arg N)`
> for a single argument

`[N]` is a **placeholder for a number**, but it is rendered inside backticks,
i.e. as code, so it reads as literal bracket syntax. The correct form is
`argv 2 a`. Measured 2026-09-14: the AI followed the message literally,
concluded the advertised form was broken, and reported a bug. DG: *"are you
putting literal brackets in the argv? I just did argv 2 a and it works"*.

Note `$(arg N)` in the same sentence uses the bare-`N` convention, so the
message is internally inconsistent about how it writes a metavariable.

## Second, smaller defect

Following the message literally does not produce a clean diagnostic:

```text
argv [2] a
echo $(len a)
```

compiles, then at runtime:

```text
sash: bug: bare command passed to sash_exec (expected absolute path)
argv: command failed (exit 127)
```

`argv` is a recognized name, so SB5011 ("command not found") does not fire,
and the malformed invocation reaches codegen as a bare command exec. An ICE
on malformed input is a compiler bug regardless of how the input got written;
it should be a compile-time rejection.

## Fix

1. Write metavariables consistently and outside code spans — `argv N a` to
   match `$(arg N)`, or a non-backtick placeholder.
2. Reject `argv` with a non-numeric first operand at validate time.

## Related

`harness-diagnostic-advice-compiles` (P3) — the general "a diagnostic's advice
must compile" gate. This is an instance where the advice *does* compile once
read correctly but is written so it invites an uncompilable reading.


## DONE 2026-09-28 — in two parts, months apart

**The second defect (the ICE) was already fixed.** `argv [2] a` is now SB5085
("`argv N a`'s index must be a bare non-negative integer literal…") rather than
a binary that dies on `sash: bug: bare command passed to sash_exec`. Pinned by
`av.f3`-`av.f7` in `spec/argv.sh`, whose comment records the same reading:
"the brackets in the docs mean 'optional', not literal — taking SB5025's own
advice verbatim used to produce a binary that ICEd". That filing
(`bug-getenv-status-unvalidated`) turned out to cover eleven intrinsics, not
two.

**The wording defect was still live**, which is the sharper half: the advice
kept inviting the reading that av.f3 rejects. Both user-facing sites now lead
with a form that compiles as written and spell the optional start bare, so the
sentence is internally consistent with the `$(arg N)` beside it:

- `argv_splat_diag` — "snapshot into an array with `argv a` (or `argv N a` to
  start at argument N), or use $(arg N) for a single argument"
- the `array a=$(argv …)` rejection — "use the out-param form `argv a`, or
  `argv N a` to start at argument N"

A sweep of every non-comment line under `sash/src/` for a `[N]`-style
metavariable found no third site. The internal doc comments still write
`argv [N] a` with brackets-mean-optional, which is fine — the gate is about
diagnostics.

Pinned by `av.f11`, which asserts both directions: the message contains
`argv a` and does **not** contain `argv [N] a`.
