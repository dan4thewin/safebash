title: Pinned

## Regex {n,m} quantifiers — BLOCKED

Plan 9 regex engine doesn't support counted repetition. Needs DG's
decision on approach. See tasks/regex-counted-repetition.md.

## [[ -v var ]] edge case

`-v` for a variable set to empty string returns false (differs from
bash where `-v x` is true after `x=""`). Noted, not fixed.

## DD#31 arithmetic contexts

`$((x=10))` is a compile error — mutation in a read context is
invalid sash. `(( ))` is for mutations, `if (( ))` is for comparisons,
`$(( ))` is for pure evaluation. See DD#31. Do not "fix" this.

## Subshell implementation vs design

The IR, transform, and codegen still implement subshells via fork/wait.
DD#38 says bash-mode subshells should be decomposed into sash primitives
(local, save/restore cwd). The fork-based implementation works but
doesn't match the design. See tasks/subshell-decomposition.md.

## Cranelift version

Current codebase uses cranelift 0.130. Newer versions have API changes
(`BlockArg::Value` wrapper, `jump`/`brif` signature changes) — explored
but not adopted.
