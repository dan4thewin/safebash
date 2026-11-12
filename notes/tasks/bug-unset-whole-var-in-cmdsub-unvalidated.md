title: "`$(unset zz)` of an undeclared whole variable is silently accepted"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`$(unset zz)` reports the same diagnostic as a top-level `unset zz`"
  - "a body-local name is NOT visible to the enclosing scope afterwards"

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

## Resolved 2026-09-29

Fixed, but not as filed. Both the diagnosis and the cost estimate were wrong.

The filing said the fix "needs a child `TypeEnv` per body, copying
`check_function`", and flagged blast radius as the risk. Measured:

- `check_function` opens no child scope — it saves/restores only `args` and the
  DD#100 slot kinds, so function bodies already share the enclosing env. There
  was no scoping to copy.
- The `structural_bodies()` pin was not the blocker. Widening it changed
  nothing, because `check_stmt` reaches that helper from its `_` arm and a
  capture is a `Declaration` — a leaf arm. Same shape as the
  `validate_stmt_ctx` bug fixed earlier the same day.
- Blast radius was **zero**: the sweep held at the 6975 baseline exactly, which
  also means the suite had no coverage of type-checking inside a body at all.

Fix: unconditional `cmdsub_bodies()` descent at the end of `check_stmt` and
`infer_stmt`; `_` arm left on `structural_bodies()` to avoid double-visiting.
Fires exactly once at depth 1-3 and for a body inside a body. DD#30's
`unset name[i]` carve-out preserved.

Specs `csc.35`-`csc.40` in `spec/errors.sh` (plan 311 -> 317). Sweep 6987.
DD#86 addendum records the corrected contract. Journal 444c.
