---
title: "native: arithmetic ternary eagerly evaluates both branches (side effects leak)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`int x=0; echo $(( x>0 ? 10/x : 42 ))` prints `42` with NO `division by 0` on stderr (matches bash and the c backend)"
  - "the untaken branch's side effects (div/mod-by-zero diagnostic, overflow trap) do not fire"
  - "oracle_tests / a spec pins native == bash for a side-effecting untaken ternary branch"
---

## Symptom

Native's arithmetic ternary is **eager** — it computes *both* branches, so a
div/mod-by-zero (or overflow) in the *untaken* branch still fires its runtime
diagnostic. bash (and, after journal 192, the c backend) short-circuit like
C's `?:`.

```
$ bash -c 'x=0; echo $(( x>0 ? 10/x : 42 ))'
42                        # lazy: 10/x never evaluated

$ ./sash -c 'int x=0; echo $(( x>0 ? 10/x : 42 ))' && ./a.out
bash: division by 0 (error token is "0")   # <-- native, spurious (stderr)
42
```

stdout matches (`42`), but native emits a stderr diagnostic bash never
produces → an **oracle divergence** for side-effecting untaken branches.

## Cause

`codegen/values.rs::lower_ternary_value` (~806-849) is straight-line
(cyclomatic = 1): it lowers `true_val` and `false_val` unconditionally and
selects between the two results (a `select`, not a branch). Any side-effecting
statement a branch emits (the div-by-zero check, the checked-arith overflow
trap) therefore runs regardless of the condition.

## Fix

Lower the ternary with real control flow, mirroring `if`: brif on the
condition into a then-block and else-block, each computing only its own branch,
merging at a block that takes the result as a block parameter (phi). The c
backend already does the lazy form (emit_c `emit_ternary_as_int_c`, an
if/else into a temp — journal 192); bring native to parity.

## Discovered

While coverage-grinding the ternary (journal 192). The c backend's ternary was
separately *broken* (empty output — no `lower_value_as_str` arm) and fixed in
the same session; this native laziness gap is the remaining half. spec
coverage.sh cov.49-56 deliberately use side-effect-free branches so they pass
on both backends until this is fixed.
