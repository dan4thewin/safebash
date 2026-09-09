---
title: "janet: `write` (DD#99 echo-without-newline) is not emitted"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "janet: `write \"a\"; write \"b\"; echo \"|\"` prints `ab|`"
  - "spec `commands.sh` wr.1, wr.2, wr.4, wr.5 pass in janet mode"
---

## Found

2026-09-09, DD#99 stage 4. `write` desugars to `echo -n` in the transform
(`stmt_handlers::echo_no_newline`), which native and c both see because they
lower from SSA. `emit_janet` renders from the **AST**, so it never sees the
desugar and emits a bare `write` command.

Registering `write` under `BuiltinCmd::Echo` stopped janet resolving it to
`/usr/bin/write`, so it no longer shells out — but it still emits `(print …)`
with the newline rather than honouring `-n`.

## Note

Related to `janet-echo-flags-and-elide` (janet's builtin echo lacks `-n`
handling): if that lands, this may follow for free. Same root as every other
janet gap this week — the front end is shared, the emitters are not, and
`emit-janet-from-ssa` would retire the class.
