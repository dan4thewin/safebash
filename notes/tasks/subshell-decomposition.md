title: Subshell decomposition — lowerer should coerce to sash primitives
status: done
depends: dd38-subshell-coercion-docs
severity: correctness
gates: [38.4, 38.5]

Currently the lowerer emits `StmtKind::Subshell` and transform/codegen
implement it as fork/wait. DD#38 says bash-mode subshells should be
mechanically coerced to sash primitives in the lowerer:

- Assignments in subshell body → wrap in synthetic function with `local`
- `cd` in subshell body → save `$PWD` to synthetic local, restore after body
- Traps in subshell body → save/restore trap state

The lowerer should analyze the subshell body, identify which isolations
are needed, and emit the appropriate IR (Group with locals, cwd
save/restore commands) instead of Subshell. Fork should only be emitted
when process isolation is genuinely required (e.g., subshell contains
`exit` that must not terminate the parent).

This makes sash-native code fork-free for the common subshell patterns
while preserving correct semantics.
