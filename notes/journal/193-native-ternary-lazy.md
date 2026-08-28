# 193 — native arithmetic ternary made lazy (bug fixed)

Follow-up to journal 192, which fixed the c backend's ternary (empty output)
and *filed* `native-ternary-eager-eval` — native's ternary evaluated both
branches, leaking a side-effecting untaken branch's diagnostic (div-by-zero,
overflow) that bash never emits. Fixed here in the same session.

## Root cause

`codegen/values.rs` had two eager ternary lowerings, both straight-line
(`select` over both branches computed unconditionally):
- `lower_ternary_value` (string/(ptr,len) context)
- the `SsaValue::Ternary` arm inside `lower_value_as_int` (int context)

A `select` needs both operands materialized, so both branches' side-effecting
statements (the div-by-zero check, the checked-arith overflow trap) ran.

## Fix

Both now use real control flow: `brif` on the condition into then/else blocks,
each lowering only its own branch, merging the result through a `merge_block`
block param (phi) — the exact pattern `codegen/arithmetic.rs` already uses for
its div-by-zero merge (`append_block_param` + `BlockArg::Value` +
`block_params(merge)[0]`). Extracted the int-context arm to a new
`lower_ternary_int` helper so the `lower_value_as_int` dispatcher stays thin.

## Result

```
$ ./sash -c 'int x=0; echo $(( x>0 ? 10/x : 42 ))' && ./a.out
42                # was: "bash: division by 0" on stderr, then 42
```

Native now matches bash (and the c backend) — the untaken `10/x` never runs.
Simple cases unchanged (1/2/10/20, nested). Regression pin: coverage.sh cov.57
(native+c: div-by-zero in the untaken branch is silent). Task
`native-ternary-eager-eval` → done.

Both backends' ternary is now correct and lazy; the ternary story (found via
the coverage grind) is complete.
