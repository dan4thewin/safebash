---
title: "`$(unset zz)` of an undeclared whole variable is silently accepted"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "`$(unset zz)` reports the same diagnostic as a top-level `unset zz`"
  - "a body-local name is NOT visible to the enclosing scope afterwards"
---

# What

```sash
str s=$(unset zz)
attest s [[ -z $s ]]
echo "[$s]"
```

compiles clean. At top level, `unset zz` for a name that was never declared is
diagnosed. Inside a `$( … )` body it is not.

# Why it survived journal 444

`check_unset_args` lives in **`type_check.rs`**, and that pass was deliberately
pinned to `structural_bodies()` when `$( )` bodies joined the statement
recursion. `check_stmt` / `infer_stmt` **mutate `env`**, so descending into a
body would declare the body's local names into the *enclosing* scope — the
exact leak DD#86 exists to prevent.

`check_function` already solves this: it opens a child `TypeEnv` for the
function body. A body descent needs the same treatment, which is why this is a
real piece of work rather than a one-token change.

# Where

- `sash/src/type_check.rs` — `check_unset_args`
- `sash/src/type_check.rs` — `check_stmt` / `check_stmt_bodies`,
  `infer_stmt` / `infer_stmt_bodies` (the pinned walkers)
- `sash/src/type_check.rs` — `check_function` (the child-scope pattern to copy)

Filed 2026-09-29, journal 444.
