---
title: "emit_c: function-local int collection (`local int array`) leaves the table var undeclared"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`fn f { local int array a=(7 8); echo ${a[1]}; }; f` compiles+runs in c mode"
  - "arithmetic.sh ivc.8 widened from native-only to native,c"
---

# Problem

`local int array a=(…)` / `local int map …` inside a function fails to compile
in c mode:

    error: 'a_ptr' undeclared (first use in this function)

The emitted C **assigns** the collection to `a_ptr` (via emit_assign, correctly
routed to the String/table path) but `a_ptr` is never **declared** — the
function's variable-declaration section declares the save slot (`__save_f_a_ptr`)
but not the working `a_ptr`.

native handles this fully (ivc.8 passes); top-level `int array` and non-local
`int array` inside a function both work in c. Only the `local` + int-collection
combination trips it.

# Notes

- This is the emit_c function-local analog of the top-level slot fix in DD#76
  (a collection-valued assignment target is a table pointer, not a scalar int).
  The top-level fix touched `collect_inst_var_types` and `emit_assign`; the
  function-local declaration/save-restore path needs the same String override
  for a collection whose declared element type is int.
- Likely near the function save/restore emission (`__save_f_<name>`) — the
  working local's declaration must be emitted as a table (`_ptr`/`_len`) when
  the local is an int-valued collection.
- Clean compile failure (gcc rejects), not a silent miscompile.

# Test

Widen the `in_modes native` guard around ivc.8 in spec/arithmetic.sh to
`native,c` once fixed.
