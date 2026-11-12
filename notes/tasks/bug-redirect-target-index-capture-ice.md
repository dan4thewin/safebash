title: "ICE: a capture inside a redirect target's subscript (`>${m[$(f)]:-d}`)"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "every program in the first table below is rejected with a diagnostic, or compiles and runs, in both backends — none panics"
  - "the forms in the second table keep their results"
  - "following the SB4030 advice for `>$m[…]` (`${m[…]:?message}`) no longer panics"
  - "specs pin the chosen behaviour in both backends, including an external capture under `--no-taint`"

## Found

2026-10-27, while porting B37 (batch-531 finding, journal 538). B37 makes
command resolution reach every `$( … )` body the checkers reach, including one
in a redirect target's subscript. The panic itself is **pre-existing**: it
occurs at base (`1309385`), on the batch-531 reference branch, and after B37,
with a different line number each time.

A redirect target whose subscript holds a command substitution passes the
parser's scalar check (SB4049) and the transform's target spill, and panics
the compiler in both backends. The native backend exits 1, C exits 101.
The q-programs start with `fn f { echo k }` and `map m=([k]=/tmp/out)`; p11
starts with the map only:

| Program | Default | `--no-taint` |
|---|---|---|
| q11f `echo hi >${m[$(f)]:-/tmp/outd}` | ICE `FuncCall (f) should have been lifted during transform` | same |
| q11q `echo hi >${m[$(f)]:?nokey}` | ICE, same | same |
| q11c `echo hi >x${m[$(f)]:-d}` (a concatenation) | ICE, same | same |
| p11 `echo hi >${m[$(cat /tmp/one)]:-/tmp/outd}` | SB5020, reported at the target | ICE `non-intrinsic command substitution must be lifted to a pipeline` |

Panic sites by tree:

| Tree | q11f / q11q / q11c | p11, native | p11, C |
|---|---|---|---|
| base `1309385` | ssa.rs:2030 | codegen/value_handlers.rs:259 | emit_c.rs:3430 |
| reference branch `e98fca8` | ssa.rs:2295 | value_handlers.rs:259 | emit_c.rs:3490 |
| after B37 | ssa.rs:2217 | value_handlers.rs:259 | emit_c.rs:3430 |

At base, p11 panics **with taint on** too: base's resolution never reached the
subscript, so `cat` kept a bare head and the taint pass read it as clean.
After B37 it is SB5020, as on the branch.

q11q is the spelling the compiler itself recommends. `echo hi >$m[$(f)]` and
`echo hi >"${m[$(f)]}"` are SB4030 ("undispositioned cell read … Use
`${m[…]:?message}`"), and following that advice panics.

These do not reach the panic, in both backends:

| Program | Result |
|---|---|
| `echo ${m[$(f)]:-d}` and `b=${m[$(f)]:?nokey}` (not a target) | run, print the mapped value |
| `echo hi >$(f)` | SB4049 |
| `v=$(f)` then `echo hi >$v` | SB4030 ("`$v` may be empty", DD119) |
| `v=$(f)` then `echo hi >${v:?no target}` | runs |
| `array a=(/tmp/out)`, `int i=0`, then `>${a[$i]:-d}`, `>>${a[$(( i + 0 ))]:-d}`, `<${a[$(( i ))]:-d}` | runs |

## Cause

`redirect_target_is_scalar` (parse/parser.rs:4125), the check behind SB4049
(`reject_nonscalar_redirect_target`, :4139), accepts any `ParameterExpansion`
or `Subscript` without looking inside its index. The transform then spills a
dispositioned target into `__redir_target_N` through `expr_to_value`
(`spill_scalar_redirect_targets`, transform/statements.rs:369), and nothing on
that path lifts a capture. So the `FuncCall` (a user function) or the raw
command substitution (an external) reaches the backend, which panics.

## Fix: a question for DG

The panic is a bug. How to fix it is a language-surface fork.

**(a) Reject (recommended).** SB4049 for a `$( … )` anywhere in a redirect
target: in a subscript, a default, or a part of a concatenation. This
extends DG's 2026-08-30 ruling in bug-redirect-nonsimple-target-empty
("support scalars, reject complex": a target is `$x`, `${x:-default}`,
`$a[i]` or a literal/quoted/concat of those; "reject a command substitution
`$(…)` or arithmetic `$((…))`"). It also matches DD119, which keeps SB4049
for command-substitution targets ("DD#119 does not weaken it"). The
work-around is one line: `k=$(f)`, then `>${m[$k]:-d}`. The SB4030 advice
above is then never reached with a capture subscript, because SB4049 is
raised at parse. p11 becomes SB4049 with taint on as well.

The ruling also rejects a whole `$((…))` target. An arithmetic subscript
inside a target is not that: it runs no command, and it works today (the
control above). Under the port's rule (DG 2026-10-24: behaviour that is not
obviously a bug stays unchanged), (a) leaves it legal.

**(b) Lift.** Treat a subscript capture in a target as one in an argument
(`echo ${m[$(f)]:-d}` works today) and lift it before the spill. That follows
DD119's reframing: the target is provably non-empty, because it has a
non-empty default. But it widens the redirect surface that DG narrowed on
2026-08-30, and it raises a new evaluation-order question: is the target's
capture lifted before or after the command's own argument captures?

A related wording question in the same function: SB4049's advice ("Assign it
to a variable first, then redirect to `$v`") predates DD119. Followed
literally, it produces SB4030, and only `${v:?message}` compiles. Should the
advice name the disposition?

## Specs to write

Specs go in the next free block of `spec/errors.sh`, or `spec/taint.sh` for
p11. Check both collision checks and the database high-water mark first. All
are dual-mode.

- q11f, q11q and q11c: under (a), SB4049 at the target. Under (b), each runs
  and writes to the mapped file.
- p11: with taint on, SB4049 under (a) and SB5020 (as now) under (b). Under
  `--no-taint`, SB4049 under (a); under (b) it runs.
- Controls: the arithmetic-subscript program and `>${v:?…}` above, unchanged.

## Related

- bug-redirect-nonsimple-target-empty (done): SB4049, DG 2026-08-30.
- DD119: a redirect target must be provably non-empty.
- bug-match-out-array-capture-operand-ice: the same `should have been lifted`
  panic, from another position that lifting misses.
- batch-531-findings.md, B37.
