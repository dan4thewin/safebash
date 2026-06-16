---
title: argv/arg/argc trio — bug-for-in-$@ closure
status: done
priority: P0
kind: bug
depends: [argv-trio-validator, argv-trio-codegen, argv-trio-converter, argv-trio-test-migration]
gates:
  - top-level `for x in "$@"; do echo "$x"; done` → SB-error (was ICE)
  - inside-function `fn f { for x in "$@"; do echo "$x"; done }; f a b c` works
  - replacement pattern `for x in $(argv 1); do bless x; echo "$x"; done` works
  - cf.f4 passes under native, c, and (if applicable) janet backends
  - bug-for-in-dollar-at-taint-ice.md updated with resolution frontmatter
  - bug-for-in-dollar-at-taint-ice.md status flipped to done with pointer to this task chain
---

# argv/arg/argc trio — bug-for-in-$@ closure

Closes [bug-for-in-dollar-at-taint-ice](bug-for-in-dollar-at-taint-ice.md)
as a consequence of T2..T6 landing.

## Pre-fix repro

```bash
$ target/release/sash -c 'for x in "$@"; do echo "$x"; done' -o /tmp/t
-c: ICE: tainted variable x_3 used as argument to command 'echo' — CST taint pass missed this
```

Reproduced 2026-06-13. The ICE is the symptom of three latent gaps
(DD46 not enforced for top-level $@ codegen; no clean sash-native
way to express "iterate user args"; DD24 + DD46 boundary unclear
for the for-loop variable).

## Post-fix behavior

After T2..T6 land:

**Top-level forms — clean SB-error redirect:**

```bash
$ sash -c 'for x in "$@"; do echo "$x"; done' -o /tmp/t
-c:1:11: error [SB-NEW] $@ is function-local — use $(argv) at top level
        (and quote-around-splat rejected per DD22)

$ sash -c 'echo "$@"' -o /tmp/t
-c:1:6: error [SB-NEW] $@ is function-local — use $(argv) at top level

$ sash -c 'echo "$1"' -o /tmp/t
-c:1:6: error [SB-NEW] $1 is function-local — use $(arg 1) at top level
```

**Inside-function — accepted, clean:**

```bash
$ sash -c 'fn each { for x in $@; do echo $x; done }; each a b c' -o /tmp/t && /tmp/t
a
b
c

$ sash -c 'fn greet { echo "$@"; }; greet alpha beta gamma' -o /tmp/t && /tmp/t
alpha beta gamma
```

**Replacement pattern at top-level — accepted, clean:**

```bash
$ sash -c 'for x in $(argv 1); do bless x [[ $x =~ . ]] ; echo $x; done' -o /tmp/t && /tmp/t a b c
a
b
c
```

## What this task explicitly does

1. Verify the gates above hold after T2..T6 land (manual probe + spec
   tests pass)
2. Migrate `bug-for-in-dollar-at-taint-ice.md` to `done` status:

```yaml
status: done
resolved: 2026-06-DD
resolution: |
  Closed by the argv/arg/argc trio task chain (argv-trio-*). Root
  cause was three latent gaps:
    - DD46 not enforced for top-level $@ codegen — bypassed the
      validator into a working-but-DD-violating splat path
    - DD42 $(argv N) shape was scalar-only — no clean way to express
      "iterate user args" at top level
    - DD22 (no word splitting) tolerated decorative quotes around
      array splats — masked the actual surface form

  Fix:
    - DD60 (new): C-semantics argv/arg/argc trio
    - DD22 addendum: quotes always yield a single string
    - DD46 enforcement at validator level
    - sash-convert rewrites top-level positionals to builtin forms
    - bug now produces a clean SB-error directing to $(argv) (top
      level) or compiles cleanly (inside function)
  resolution-spec-tests: cf.f4, cf.dd46.6, cf.f11..f14
gates:
  - top-level "$@" SB-error pass ✓
  - inside-function "$@" works ✓
  - replacement pattern $(argv 1) + bless works ✓
  - cf.f4 passes ✓
```

3. Update spec/control_flow.sh cf.f4 from the original (failing,
   ICE-producing) form to the post-fix form:

   Original (failing):
   ```bash
   # cf.f4 for over $@ (positional params passed via argv)
   run_sash 'for x in "$@"; do echo "$x"; done' alpha beta gamma
   [[ $rc = 0 && $out = $'alpha\nbeta\ngamma' && -z $err ]]; chk cf.f4
   ```

   Post-fix:
   ```bash
   # cf.f4 for over user args via $(argv 1)
   run_sash 'for x in $(argv 1); do bless x [[ $x =~ . ]]; echo $x; done' alpha beta gamma
   [[ $rc = 0 && $out = $'alpha\nbeta\ngamma' && -z $err ]]; chk cf.f4
   ```

## What this task explicitly does NOT do

  - It does NOT implement the validator/codegen/converter changes —
    those are T2..T5.
  - It does NOT migrate other tests — T6.
  - It does NOT write the journal entry — that lands when the whole
    chain (T1..T7) is done.

## Verification protocol

Before flipping status to done:

1. Run `mk test` — all tests pass (`sash/tests/`)
2. Run `cd spec && bash run.sh` — spec.db updates
3. Query: `sqlite3 spec.db "SELECT id, status FROM test_status WHERE
   id IN ('native:cf.f4','c:cf.f4','native:cf.dd46.6','c:cf.dd46.6')"`
   — all `passing`
4. Probe the three forms in the "Post-fix behavior" section manually
   and confirm output matches
5. Update bug-for-in-dollar-at-taint-ice.md per template above
6. Commit with message linking this task and the bug file

## References

  - [bug-for-in-dollar-at-taint-ice](bug-for-in-dollar-at-taint-ice.md)
  - [argv-trio-design](argv-trio-design.md)
  - [argv-trio-validator](argv-trio-validator.md)
  - [argv-trio-codegen](argv-trio-codegen.md)
  - [argv-trio-converter](argv-trio-converter.md)
  - [argv-trio-test-migration](argv-trio-test-migration.md)
  - AGENTS.md "Bug policy" — zero tolerance, ICE is a stop-everything
    event
