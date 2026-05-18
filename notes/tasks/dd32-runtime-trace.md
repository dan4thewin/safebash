title: DD#32 — implement SASH_TRACE runtime trace
status: done
depends: []
gates:
  - tests
  - commit

`SASH_TRACE=1` should produce a runtime trace showing evaluated values
and branch outcomes. DD#32 status is partial — embedded source is
implemented, runtime trace is designed but not built.

Trace output to stderr. Show:
- Variable assignments with evaluated values
- Branch conditions with outcomes (true/false)
- Function calls with arguments
- Command executions with expanded arguments

Gated on `SASH_TRACE` env var check at startup. Zero overhead when
disabled (check once, store flag, branch in codegen).

Ref: DD#32
