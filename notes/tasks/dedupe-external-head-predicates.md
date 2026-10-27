---
title: "Dedupe the external-head predicates: four functions and two name lists answer one question"
status: open
priority: P4
kind: hardening
depends: []
gates:
  - "one predicate decides whether a literal command head runs an external program, for the validator's taint walk, the transform's clean-pipe test and SB4025's capture classification"
  - "one list of compiler-owned names: `BUILTINS` is derived from `resolve::is_intrinsic`, or a unit test pins both inclusions"
  - "a full sweep has no flips"
---

## Found

2026-10-27, while porting F1+F4 (journal 537). The port split
`is_external_source`'s head test out into `source_head_execs` for the
complexity gate, which put the two predicates side by side. Four
functions ask whether a command head runs an external program:

| Function | Test | Used by |
|---|---|---|
| `validate::source_head_execs` (6035) | starts with `/`, or non-empty and not in `BUILTINS` | `is_external_source`: the validator's `taint_pipeline` (6870) and the transform's clean pipe (stmt_handlers.rs 931, 1006) |
| `resolve::is_external_looking` (395) | contains `/`, or not `is_intrinsic` | `validate::is_external_command_head` (1373): SB4025's capture classification |
| `resolve::is_resolvable_external` (347) | no `/`, not `is_intrinsic`, not a user function | `resolve_command` |
| `validate::is_valid_command` (4439) | a path, in `BUILTINS`, a user function, or on PATH | SB5011 |

They agree today on every head that can arrive:
- a relative head is external to both of the first two, and resolution
  rejects it (SB5051);
- an empty literal head, the one place `source_head_execs` and
  `is_external_looking` differ, is SB4074 at parse (`qhead.10`-`qhead.12`);
- a non-literal head is "not external" to both (`expr_literal_str` returns
  `""`; `is_external_command_head` matches only a literal).

They run on both sides of DD#23 resolution. SB4025's classification runs in
the structural `validate` pass, which `run_structural_checks` calls before
`resolve_commands` (lib.rs 479, 484), so it sees bare names and has to judge
them by name. The taint pass and the transform run after resolution, when
every external head is already absolute. There, the bare-name half of
`source_head_execs` only ever catches a user function (B35, below). A shared
predicate has to serve the pre-resolution caller.

Behind them are two hand-maintained lists. `BUILTINS` (validate.rs
3758-3827) has 44 names; `is_intrinsic` is `BuiltinCmd::from_name` (ssa.rs
190-228, 32 names) plus `INTRINSICS` (resolve.rs 59-72, 12 names). They are
equal today, computed 2026-10-27, but only one direction is tested:
`every_builtin_name_is_an_intrinsic` (validate.rs 11718) checks `BUILTINS`
⊆ `is_intrinsic`, and `value_only_intrinsics_are_all_builtins` (11737) checks
a third list against `BUILTINS`. The tested direction is the dangerous one:
`kill` sat in `BUILTINS` without being an intrinsic, so `resolve_command`
ran `/usr/bin/kill` while taint called it non-external, the divergence
`is_external_source`'s doc calls a laundering hole (removed 2026-10-10; the
test's doc). The untested direction would make `source_head_execs` call a
compiler-owned name an external source: a false SB5020 rather than a hole,
but still a second answer.

## Fix direction

Give `resolve` one predicate for a literal head and have
`is_external_source` and `is_external_command_head` call it. Replace
`BUILTINS` with `resolve::is_intrinsic`, or keep it and test the reverse
inclusion. No verdict should change.

## Not this task

None of the taint-side predicates knows the user's functions, so
`source_head_execs` calls a function stage an external source:
`fn f { echo hi }; f | readline x` is a false SB5020. That is B35, which
the port plan lands with W2 (T1 stdout; `batch-531-findings` §18). Its fix
gives the classifier the function table, so it is simpler on one shared
predicate. Do this first or with W2.

The branch's walker (`taint-walker-redesign`, `e98fca8`) already drops that
bare-name half. Its `is_external_source` asks `command_execs`: a head
containing `/`, `exec CMD`, or a function that reaches exec. That relies on
resolution having run, and it is W2's to port. The other three predicates
and both lists are unchanged on the branch.
