title: argv/arg/argc trio — codegen + runtime phase
status: done
priority: P0
kind: feature
depends: [argv-trio-validator]
gates:
  - $(argc) returns C argc (was bash $#) — existing callsites & tests updated
  - $(arg N) lowers as scalar via sash_argv_get (existing runtime), 0-indexed
  - $(argv) and $(argv N) lower as array splats (slice from index N)
  - splat path unifies with the existing $@ in-function path where natural
  - av.t1..t7, av.s1..s4, cf.f11..f14 pass under native and c backends
  - no regressions in existing 291 native spec tests

# argv/arg/argc trio — codegen + runtime phase

Wires the IR shapes recognized by T2 into actual codegen. Builds on
the existing argv machinery in `sash-rt/` and the for-loop splat
infrastructure in `sash/src/transform/pipelines.rs`.

## A. Fix $(argc) to return C argc

Today `$(argc)` returns 3 for `./bin a b c` (matches bash `$#`).
Required: returns 4 (C argc, includes program name).

Implementation candidates:
  1. Compile-time substitution: `$(argc)` lowers to a load of
     `sash_argc` directly (no `- 1`). Check the current lowering
     site (likely `value_handlers.rs` near the argv handler).
  2. Adjust the runtime helper that backs `$(argc)` if there's a
     dedicated one.

Likely a one-line change. Coordinate with T6 to update any test
that asserted the bash-$# value.

## B. Implement $(arg N) (scalar)

`$(arg N)` is morally identical to today's `$(argv N)` scalar handler
at `value_handlers.rs:946`. Lowers to `sash_argv_get(N, &out_len)`.
The handler exists; T2 added validator-side recognition for `arg`.
T3 wires the codegen dispatch to share the existing path.

Out-of-range abort: `sash_argv_get` already aborts. Keep behavior.

## C. Implement $(argv) and $(argv N) (array splat)

This is the largest piece. Three contexts to handle:

**C.1 for-in header**

  for x in $(argv);    do ...; done    # iterate index 0..argc-1
  for x in $(argv 1);  do ...; done    # iterate index 1..argc-1

Builds on `lower_for_dollar_at` at `sash/src/transform/pipelines.rs:154`
(which already iterates argv via internal index counter __fi). The
existing implementation iterates from index 1 (today's `$@` semantics,
matching bash `$@` which excludes argv[0]). The new `$(argv)` form
iterates from index 0; `$(argv N)` iterates from index N.

Refactor candidates (decide in T3's first commit):
  1. Parametrize `lower_for_dollar_at` with a start-index argument.
     Today's $@ lowering passes 1; new $(argv) passes 0; new $(argv N)
     passes N.
  2. Rename to `lower_for_argv_slice` and have the in-function $@
     path also dispatch through it.

**C.2 array-snapshot RHS**

  array a=$(argv)
  array a=$(argv N)

Handled in T4 (separate task with its own gates).

**C.3 command-arg position**

NOT IMPLEMENTED — per DD60, `$(argv)` in command-arg position is
rejected by the validator. The inside-function `$@`-in-command-args
path (`emit_exec_with_dollar_at` at `codegen/dispatch.rs:1097`)
stays for that case only, gated by being inside a function body
(T2's validator already enforces this).

## D. Splat path unification

Today there are three near-parallel splat paths:

  - `lower_for_dollar_at`   (transform/pipelines.rs:154, 126 lines)
    — iterates argv via __fi
  - `lower_for_array`       (transform/pipelines.rs:9, 143 lines)
    — iterates named array via __fi
  - `emit_exec_with_dollar_at` (codegen/dispatch.rs:1097, 11 lines)
    — splices argv into a command's argv buffer

After T3, these should collapse where possible:

  - `lower_for_dollar_at` + `$(argv)` + `$(argv N)` → one
    parametrized "iterate argv slice" path
  - `lower_for_array` stays — semantics are different (named array,
    not argv)
  - `emit_exec_with_dollar_at` → renamed to reflect "inside-function
    $@ → splice caller args" (the only case it serves after T2)

Decide unification scope in T3's first commit; not all unification
is mandatory if friction is high.

## E. Removal of decorative-quote unwrap

`sash/src/transform/control_flow.rs:339` unwraps `DoubleQuoted` of
a single splat. T2 added validator-side rejection of these. T3
removes the unwrap arm once T6 has migrated tests off the quoted
spelling.

Sequencing:
  T2 (validator) → T6 (test migration) → T3 (remove unwrap arm)

OR coordinate as a single PR: T2's validator rule, T6's migrations,
T3's unwrap removal all land together.

## Runtime side (`sash-rt/`)

Already in place:
  - `sash_argc`, `sash_argv` globals (`sash-rt/src/lib.rs:94`)
  - `sash_argv_get(idx, &out_len)` (`sash-rt/src/lib.rs:362`)
  - argv set up via the prelude assembly (`sash-rt/src/lib.rs:565`)

No new runtime symbols expected. T3 reuses the existing helpers.

## Test gates

After T3 lands:
  - av.t1..t7 pass (scalar trio + abort)
  - av.s1..s4 pass (splat + slice + out-of-range empty)
  - cf.f11..f14 pass (inside-function positionals — already work
    today for $@/$#, may need cleanup for $1..$N)
  - 291 baseline tests pass

Tests that pass after T2 but become more meaningful after T3:
  - cf.dd46.* (validator-only gates) — already passing after T2
  - av.m1..m3 (validator-only gates) — already passing after T2

## References

  - [argv-trio-design](argv-trio-design.md)
  - [argv-trio-validator](argv-trio-validator.md)
  - [argv-trio-array-snapshot](argv-trio-array-snapshot.md) — T4
  - `sash/src/transform/pipelines.rs:154` — existing $@ for-loop splat
  - `sash/src/codegen/dispatch.rs:1097` — existing $@ command-arg splat
  - `sash-rt/src/lib.rs:362` — `sash_argv_get`
