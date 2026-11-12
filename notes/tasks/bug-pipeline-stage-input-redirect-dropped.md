title: "bug: an input redirect on a pipeline STAGE is silently dropped"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`/usr/bin/tr '\\n' '\\0' < f | newread a` fills `a` from `f` (today: empty, no diagnostic)"
  - "`/bin/cat < f | /bin/cat` prints f's contents"
  - "a redirect that cannot be honoured on a stage is a compile error, never a silent empty read"

## Found

2026-09-09, checking DD#99's line-splitting idiom. DG: *"it's 1 line:
`tr \n \0 < f | newread a`"* — and it is, except the redirect never reaches
`tr`.

```sh
/bin/cat < f.txt                              # x y z   ✓
/bin/cat < f.txt | /usr/bin/tr '\n' '\0' | …  # nothing  ✗
/bin/cat f.txt   | /usr/bin/tr '\n' '\0' | …  # x y z   ✓  (workaround)
```

With stdin attached to a terminal the program **hangs** (the stage waits on
stdin that was supposed to be redirected); with `</dev/null` it silently reads
nothing and the consumer sees an empty array. Either way there is no
diagnostic.

`/bin/grep x < f.txt | /bin/cat` confirms it independently: grep exits 1
(matched nothing) because it received no input.

## Scope

A redirect on a **non-pipeline** command works. It is specifically a stage of a
pipeline that loses it. `push_command_stage` does collect a stage's redirects
(and `spill_scalar_redirect_targets` handles their targets), so the loss is
somewhere between there and the child's fd setup — worth checking whether the
stage's redirect list survives into `SsaInst::Pipeline` and is applied after
the pipe fds are wired, rather than before and then overwritten.

## Why it matters now

This is the canonical DD#99 idiom for converting a separator to NUL —
`tr SEP '\0' < file | read arr` — and for the many `mapfile` sites that stage 5
must migrate. The workaround (`cat file | tr … | read arr`) costs an extra
fork, so the redirect form is the one worth having.

Independent of DD#99: it is a plain redirect bug that predates it.

## Resolution (2026-09-09)

Root cause: `emit_one_stage_redirect` (codegen/inst_handlers.rs) hardcoded the
implicit fd to **1**, so an input redirect on a stage was applied to that
stage's *stdout* — the child never got the file on fd 0. It then read whatever
the pipe or the inherited terminal gave it, which is why the failure looked
like a hang under an interactive stdin and an empty read under `</dev/null`.
The `open(2)` mode was already correct (`Read` takes the `_ => 0` arm).

The non-pipeline path had this right all along via `default_fd_for_op`, which
picks the fd from the redirect's *direction*. The fix makes the stage path call
that same function, so the two cannot drift apart again.

Specs 14.32-14.36 in `spec/commands.sh` (native + c): a first stage's input
redirect, a middle stage's, the DD#99 `tr … < f | newread a` idiom, the output
direction still landing on fd 1, and the non-pipeline case as a control.

Not reproducible in janet mode — `emit_janet` renders pipelines from the AST
and does not share this path.
