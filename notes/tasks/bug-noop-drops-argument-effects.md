---
title: "Bug: `:` drops its arguments' effects — `: ${s:?msg}`, `: @{a:?}` and `: $(( 1 / z ))` never abort"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "q1i, nc4, nc8, nc9, nc3, sp5, sp6 and sp9 abort in both backends, with the message and status their `echo` or `/bin/echo` twin gives (nc5, nc7, sp4, sp8, sp10, sp11)"
  - "q1h aborts as sp1 does, or is a compile error if `q-suppression-and-stage-residuals` Q1 (a) lands first"
  - "the controls are unchanged: sp7 prints `after [x]`, nc1 prints `[5]`, nc6 prints `after`, `: >f` still truncates f, and `cmd || :` still discards"
  - "cv1, converted, aborts with its message, as bash does"
  - "a spec pins each row in both modes, and cv1 in convert.sh"
---

## Found

2026-10-27, while porting F1+F4 (journal 537), probing Q1 of
`q-suppression-and-stage-residuals`. It is **pre-existing**: every row is
the same at `8e28c61`, with F1+F4, and on `taint-walker-redesign`
(`e98fca8`), in both backends.

| Id | Program | Result |
|---|---|---|
| q1i | `str s=""` then `: ${s:?empty}` then `echo after` | `after` |
| nc4 | q1i with `: "${s:?empty}"` | `after` |
| nc8 | `array a=()` then `: @{a:?}` then `echo after` | `after` |
| nc9 | `array a=(x)` then `: ${a[3]:?missing}` then `echo after` | `after` |
| nc3 | `int z=0` then `: $(( 1 / z ))` then `echo after` | `after` |
| q1h | q1i with `- : ${s:?empty}` | `after` |
| sp5 | q1i with `if : ${s:?empty} { echo y }` | `y`, `after` |
| sp6 | q1i with `: ${s:?empty} >/dev/null` | `after` |
| sp9 | q1i with `: ${s:?empty} \| /bin/cat` | `after` |

The same arguments abort everywhere else, in every one of those positions:
- nc5 `echo ${s:?empty}` and nc7 `/bin/echo ${s:?empty}` exit 1 with
  `./bin: empty`;
- with `-`: sp1 (`echo`), sp2 (`/bin/echo`); the `-` discards a status, not
  an argument's abort;
- in a condition: sp8 (`/bin/echo`), sp11 (`echo … >/dev/null`);
- as a stage: sp10 `/bin/echo ${s:?empty} | /bin/cat` prints `./bin: empty`
  and then the stage failure;
- arithmetic: sp4 `echo $(( 1 / z ))` exits 2 with
  `./bin: division by zero`, and sp3 with `-` the same.

Controls: sp7 (`s=x`) prints `after [x]`. nc1 `: $(g)`, where `g` sets a
global `n=5` and echoes, prints `[5]`, so a capture argument does run. nc6
`: $s` prints `after`.

**The converter.** cv1.bash is bash's parameter-assertion idiom: `x=""`,
`: "${x:?x must be set}"`, `echo after`. Bash prints `x: x must be set` and
exits 1. `sash-convert` passes the line through unchanged (cv1.sh), and the
compiled result prints `after`. Every converted script that guards a
variable this way loses the guard without a word.

Probes: `files/port-f1f4-probes.tar.gz`, directories `t6b/` and `t12/`
(t12 also holds nc1's emitted C).

## Why it is a bug

- DD#77's resolved sub-decisions (2026-08-17): "`:?` un-prune scope —
  everywhere"; in word context `${s:?msg}` aborts on unset or empty.
- DD#69 §2: `@{a:?}` aborts on an empty array.
- DD#86's ruling (c) under implementation phase 4 calls `: $v` "a real
  read", and DD#98 "itself a use"; the
  dead-variable gate and the taint backstop already treat `:`'s arguments
  as reads.
- DD#69 §3's "`:` ignores its args, so this is moot for it" is about the
  splat-join rule: `:` prints nothing, so `: @a` flattens nothing. It does
  not say the arguments go unevaluated.
- `emit_noop`'s doc and `ssa.rs` 249-252 record the same shape for
  redirects: `: >f` "used to *silently drop*" its redirect, and DG ruled
  (2026-10-11) to make it work.

## Cause (read, not yet confirmed by a fix)

- Native `emit_noop` (codegen/dispatch.rs 422-443) never lowers its
  arguments. Inside the redirect guard it only sets the test result and
  `$?`.
- C's arm (emit_c.rs 2094-2097) writes only `__exit_status = 0;` and
  `__test_result = 1;`, and the stage arm (4162-4164) only `sash_exit(0)`.
  nc4's emitted C has no `sash_require_value`, which nc5's has.
- An abort or a division lives inside the operand value, so it is dropped
  with the operand. A capture runs because it is lowered before the command:
  in nc1's C, the capture of `g` (lines 120-142) precedes the `:` arm
  (143-144).

## Fix direction

The DDs above settle it (decide-and-report): `:` evaluates every argument
for its effects, as `echo` does, and discards the values. That is bash's
behaviour too, so the converter's pass-through becomes correct with no
converter change. Lower the operands through the path `echo` uses, minus
the write, in both backends and in the stage arms.

The alternative, rejecting an argument whose evaluation can abort, would be
a new rejection of bash's own idiom and would need a converter rewrite with
no sash spelling to rewrite to. It is recorded so DG can veto.

Related: Q1 of `q-suppression-and-stage-residuals` (`-` on `:` discards
nothing, with or without this fix).

## Fixed 2026-10-28 (F2 port, commit B)

As the fix direction says, in both backends and all four sites: native
`emit_noop` and `lower_pipeline_builtin`'s `:` stage, C's plain and stage
`Noop` arms. A shared effects-only helper per backend
(`codegen::commands::evaluate_args_for_effects`, `emit_c::emit_args_for_effects`)
resolves the splat dispositions and lowers each other value in the argv
builder's order, then drops the results. It builds no argv, so it takes no
heap and runs no DD#99 NUL check (DD#69 addendum 2026-10-28).

Specs, both modes: noop.1-noop.9 are the rows (q1i, nc4, nc8 with a
message, nc9, nc3, sp5, sp6, sp9, and a capture-lifted stage, which reaches
the plain arm in the stage child). noop.10-noop.13 are the controls (sp7,
nc1, nc6 and the NUL control). cv1 is convert.sh 57.192. The `: >f` and
`cmd || :` controls are existing specs (errors.sh rdr.23/rdr.24, regex.sh
63.2d), unchanged in the sweep. Negative
control: noop.1-noop.9 and 57.192 fail on the pre-fix binary, the four
controls pass.

q1h (`- : ${s:?empty}`) now aborts as sp1 does, both backends, but no spec
pins it: DG ruled 2026-10-28 that nothing may cement the current suppression
story, and `q-suppression-and-stage-residuals` Q1 may make it an error.
