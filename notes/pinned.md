title: Pinned

## Regex {n,m} quantifiers — BLOCKED

Plan 9 regex engine doesn't support counted repetition. Needs DG's
decision on approach. See tasks/regex-counted-repetition.md.

## [[ -v var ]] edge case

`-v` for a variable set to empty string returns false (differs from
bash where `-v x` is true after `x=""`). Noted, not fixed.

## DD#31 is intentional

`$((x=10))` does NOT update `x`. Mutations and comparisons in `(( ))`
are disjoint. Do not "fix" this.

## Cranelift version

Current codebase uses cranelift 0.130. Newer versions have API changes
(`BlockArg::Value` wrapper, `jump`/`brif` signature changes) — explored
but not adopted.
