---
title: "`echo @a` array splat: reject with a clean error directing to `echo $(join @a)` (currently ICEs)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`array a=(one two three); echo @a` is a clean compile error (new SB code), NOT an ICE panic"
  - "the error directs to the explicit join: `echo $(join @a)`"
  - "`echo $(join @a)` compiles and prints the joined string; `echo a b c` (literals) still prints `a b c`"
  - "the reject fires for any array splat in `echo` arg position (bare or mixed, e.g. `echo x @a y`)"
  - "native + c + janet identical; spec (positive `$(join @a)` + negative bare `@a`); 0 regressions"
---

# `echo @a` → reject, direct to `echo $(join @a)` (DD#69 §3)

> **DONE (2026-07-07, autopilot).** New front-end code **SB5030** in
> `validate::check_echo_splat` (a `StmtKind::Command` arm in
> `check_argv_splat`): a named-array splat (`@a` / `${a[@]}`, an `arrvals`/
> `arrkeys` ParameterExpansion) in an `echo` argument is a clean compile
> error before codegen, so the `values.rs:61` ICE backstop is never hit.
> `$(argv)` stays SB5025 (no double). External-command splats unchanged.
> spec/types.sh 33.14 (bare→SB5030), 33.15 (`echo $(join @a)` positive),
> 33.16 (mixed→SB5030); all pass native/c/janet, 0 regressions.

Found 2026-07-04. Resolved by DG as **B** (2026-07-04): `echo @a` is a
compile error, not a splat. See DD#69 §3.

## Repro (current — an ICE)

```
$ sash -c 'array a=(one two three); echo @a' -o /tmp/b
thread 'main' panicked at sash/src/codegen/values.rs:61:
ICE: ArraySplat (@a) must be expanded by the argv builder, not lowered as
     a scalar value
```

## Decision (DD#69 §3, DG 2026-07-04)

`echo`'s arg list is a **string-output sink** (it space-joins its args to
stdout), so `echo @a` is an *implicit array→string join* — forbidden by
DD#22 (lossy for spacey elements; reintroduces word-splitting downstream).
**Reject it**; the user writes the explicit join **`echo $(join @a)`** (the
`join` intrinsic, DD#41/DD#60).

This is **not** the "make `echo` splat to N args" fix originally sketched —
that was superseded by B. External commands keep the splat (structured
argv, with the DD#69 §2 disposition); only `echo` (a join-and-print
builtin) rejects.

## Fix direction

Front-end reject (span-bearing), *before* codegen — so the `values.rs:61`
panic is never reached:

- In the array-splat validator (`check_argv_splat`, `sash/src/validate.rs`
  ~1537 — the SB5025/5026/5027 family) or command validation, detect an
  array-splat expr in an argument of a command that resolves to the inline
  builtin `echo` (`transform/resolution.rs:11` lists `echo|true|false|:`)
  and emit a new SB code (next free in the splat family; keep adjacent).
- Message: "`echo` joins its args into one output stream — an array splat
  `@a` there would flatten it (DD#22). Use an explicit join: `echo
  $(join @a)`."
- `true`/`false`/`:` ignore args; a splat to them is moot (fold in or leave).
- Validation is shared across backends, so one front-end check covers
  native/c/janet; verify none still reaches the ICE.

## Notes

- The `values.rs:61` panic is a correct backstop — the fix makes it
  unreachable for `echo`, not softens it.
- Negative spec (bare `echo @a` → new SB code) paired with a positive
  (`echo $(join @a)`), per the bless-negative-test convention.

## Refs

- `notes/decisions/DD69-globs-as-arrays-splat-disposition.md` §3 (decision
  + the printf/builtin-exemption correction).
- `join` intrinsic: DD#41 (`len`/`keys`/`join`), DD#60:101.
