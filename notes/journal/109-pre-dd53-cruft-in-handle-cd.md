---
title: Pre-DD#53 cruft in handle_cd (and elsewhere)
date: 2026-06-07
---

# Pre-DD#53 cruft in handle_cd (and elsewhere)

While refactoring `handle_cd` for cog (was 20), DG asked whether the
function should even contain magic-variable logic given DD#53 (env
isolation). It shouldn't. Triage and excision below.

## What was there

`handle_cd` had three branches, all built around magic-var semantics
that DD#53 eliminated:

1. **`cd -`** read `vars.get("OLDPWD_0")`. Under DD#53, `$OLDPWD` is a
   plain sash variable that is empty unless explicitly assigned, NOT
   the env var. So `cd -` silently went to the empty string (chdir
   failure → `$?=1`) unless the user happened to have a sash variable
   named `OLDPWD`. There is no DD that defines `cd -` semantics.
2. **No-arg `cd`** read `vars.get("HOME_0")` — same problem.
3. **Both arg paths** rewrote `PWD_0` and `OLDPWD_0` (when present) to
   reflect `getcwd()` and the previous `PWD` value. This **silently
   overwrote user state** — if a script did `PWD=somepath`, the next
   `cd` would clobber it. No DD supports this.

Compounding it: `validate.rs:388` already rejects bare `$HOME`,
`$PWD`, `$OLDPWD` etc. with SB5018, directing the user to
`$(getenv NAME)`. So the slots `HOME_0`/`PWD_0`/`OLDPWD_0` can only be
populated by an explicit `HOME=...` / `PWD=...` / `OLDPWD=...` sash
assignment — at which point `handle_cd`'s magic behavior is overwriting
the script's deliberate state.

## How it survived

The `brush_cd_pwd` test case in `e2e_tests.rs` had a docstring
admitting "codegen doesn't update $PWD after cd yet — test the cd
works". The test does `cd /` then `pwd`, expecting `/`. `pwd` uses
`getcwd(2)` and is independent of `$PWD`, so the test passed
regardless of the magic-var logic. The comment was wrong (the code
did try to update `$PWD`) and obscured the real situation.

## What landed

`handle_cd` is now a straight-line function: `chdir(arg)` + set `$?`
from the result. No-arg is a silent success (no `$HOME` magic).
`cog 20 → 0, cc 10 → 1`. Sixteenth cohort violator cleared (commit
`586c2b4`).

Test comment in `brush_cd_pwd` updated to reflect DD#53 semantics.

## Related cruft found, not yet fixed

`parse/expr.rs:142,149` lowers `~` and `~/path` to
`SimpleExpansion("HOME")` / `Concat([SimpleExpansion("HOME"), ...])`.
That means the parser hands the validator an expression containing
bare `$HOME`, which SB5018 rejects with the message
"use `$(getenv HOME)`" — confusing if the user wrote `~`. Filed as
task `tilde-vs-dd53`.

## Lesson

Whenever a handler reads `vars.get("<NAME>_0")` for a name that's also
in `validate.rs::ENV_VARS`, it's almost certainly pre-DD#53 cruft.
Search for that pattern across the codebase periodically.

```bash
grep -rn 'vars\.get("\(HOME\|PATH\|USER\|SHELL\|PWD\|OLDPWD\|LANG\)_0"' sash/src/
```

Currently zero matches — `handle_cd` was the only place.
