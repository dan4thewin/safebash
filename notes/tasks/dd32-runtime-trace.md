title: DD#32 — implement SASH_TRACE runtime trace
status: done
depends: []
gates:
  - tests
  - commit

`SASH_TRACE=1` should produce a runtime trace showing evaluated values
and branch outcomes.

> **Done for branch/loop outcomes** (`--trace` + `SASH_TRACE=1`, native
> backend, specs `tr.1`-`tr.3`). The text below describes the original
> scope; the *evaluated values* half is still unbuilt and is what keeps
> DD#32 at `partial`. It was left reading "designed but not built" long
> after the branch half shipped — corrected 2026-09-13.

Trace output to stderr. Show:
- Variable assignments with evaluated values
- Branch conditions with outcomes (true/false)
- Function calls with arguments
- Command executions with expanded arguments

Gated on `SASH_TRACE` env var check at startup. Zero overhead when
disabled (check once, store flag, branch in codegen).

Ref: DD#32
