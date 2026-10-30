# 548 - a `-` discards one status

2026-10-30. F6 unit B (Order item 13). DG ruled on 2026-10-29
(`q-suppression-on-user-functions` Q1 and Q4) that a `-` on a call discards
only the function's returned value, and that a statement's `-` does not
reach its argument captures. Taken with DD86 (a body is a function body,
checked once), that gives one rule for every `-`: it discards the status of
the one command or call it prefixes, and nothing else. The DD55 addendum of
2026-10-30 (unit B) states the rule; this entry records how it landed. The
unit asked DG nothing. The reach list is decide-and-report, and the shapes
the rule leaves open went to the backlog (DG 2026-10-30).

## Before and after

`fn f { /bin/false; echo x; return 1 }; - f; echo done` printed `x` and
`done`, because the `-` silenced the `/bin/false` inside f as well as f's
`return 1`. It now aborts at the `/bin/false`. f's `return 1` is still
discarded (noreach.2, the control). The same holds wherever a body is
entered under a `-`, and wherever a capture sits among the suppressed
command's operands:

| id | shape |
|---|---|
| noreach.1 | the called function's body |
| noreach.3 | an int function's body |
| noreach.4 | a background job's body (the child aborts; waitstatus says so) |
| noreach.5 | `$(- f)` on a status function |
| noreach.6 | a function stage of a capture condition (the condition is false) |
| noreach.7 | an external command's operand capture |
| noreach.8 | a builtin's operand capture |
| noreach.9 | an operand capture of `$(- cmd …)`'s command |
| noreach.10 | a function captured as an operand |
| noreach.11 | a `[[ ]]` operand capture |
| noreach.12 | a value intrinsic's argument capture |
| noreach.13 | a stage's operand capture in a capture condition |
| noreach.14 | a capture in an arithmetic test's operand |
| noreach.15 | an int call assigned inside an arithmetic test |
| noreach.16 | an int call as an arithmetic test's operand |

Six existing specs pinned the old reach: supval.7, .8, .18, .19, .20 and
.21. Each printed the output of a body whose `/bin/false` the `-` had
silenced. Their programs are unchanged, and their assertions now expect
the abort. Q1's ruling decides them, so they keep their ids (the F14
precedent).

## Mechanism

The old `SuppressionScope` ran after a suppressed statement was lowered,
and unchecked every block allocated in the meantime. An inlined body and
an operand capture are lowered inside that window, so the scope reached
both. It is deleted, along with `uncheck_block_from`.

The transform now keeps a suppression depth, `state.suppressed`, on
`VersionState`. `lower_suppressed_stmt` raises it around the command it
lowers. Every site that builds a status check reads `state.suppressed == 0`
as it builds (stmt_handlers, cmd_handlers, control_flow, pipelines). Two
places zero the depth:
- `functions::unsuppressed` takes it, runs a closure and restores it. Every
  operand and argument lift runs through it (`lift_test_captures`,
  `lift_int_func_args`, the value arm of `lift_suppressed_capture`), and so
  does the operand lowering of a `(( ))` test (`lower_arith_test_stmt`);
- `lower_inlined_body` takes it for every body it inlines.

`$(- cmd …)` on an external does not raise the depth. It passes a flag to
`lift_cmdsub`, which leaves that one capture unchecked
(`checked: !suppressed && state.suppressed == 0`) and lifts its arguments
at depth zero. A body capture (a statement builtin, or a capture whose
arguments run value calls) raises the depth around its own lowering, and
its operand lifts clear it again.

## Negative controls and stubs

On the pre-unit binary (1327587d), the 16 new ids and the 6 rewrites give
21 red ids per mode. noreach.2 stays green, as a control should. Three
reversion stubs, each run in both modes:

| stub | red ids |
|---|---|
| (c) no take in `lower_inlined_body` | noreach.1, .3, .4, .6, .15 |
| (b) no `(( ))` operand barrier | noreach.14 |
| (a) no take in `unsuppressed` (so no barrier) or in `lower_inlined_body` | (b)'s and (c)'s, plus supval.21, noreach.7, .8, .10, .11, .13, .16 |

The rest are supval.7, .8, .18-.20 and noreach.5, .9, .12, the `$(- X …)`
shapes. No single stub turns them red, because the flag and the scope's
deletion carry them together. The pre-unit binary is their control.

## Gate

`NEW=32 FLIPS=12 mk gate` failed one check: flips 0, expected 12. The
prediction was wrong, not the tree. A spec rewritten in the same commit as
its behaviour change passes before (old text, old binary) and after (new
text, new binary), so it is not a flip. All 12 rewritten rows (6 ids × 2
modes) read passing, then passing. Unit A's 31 rewrites gated with 0 flips
for the same reason, and I should have predicted from that. So for rewrites
that land with their behaviour change, predict FLIPS=0.

`NEW=32 FLIPS=0 TOTAL=10401 bash contrib/gate.sh --recheck` passed on the
same block without sweeping again: rows 6544014-6554414, 10401 = 10388 + 13
skip (native 5311, c 5090), 32 first-ever ids, 0 retired, 0 ghosts,
liveness 260, static checks clean. `mk gate` runs `status-update.sh` only
after a pass, so it was run by hand after the recheck.

After the sweep, 11 comment lines in the code changed their citation from
the port unit to "DD#55 2026-10-30" (committed code cites DDs). The rebuilt
binary is byte-identical (`cmp`) to the swept one.

## What the rule leaves open

Five shapes, measured on the unit B binary in both backends, are
`q-suppression-reach-residuals` (P3, question), each with a repro and a
recommendation:
- R1: an assignment's own captures stay suppressed (`- x=$(cmd)`,
  `- m[$(cmd)]=v`, `- x=$(strlen $(cmd))`);
- R2: the store in `- (( n = … ))` is lowered as a suppressed assignment,
  so a capture in its value is too;
- R3: `$(- toupper abc)` has no status to discard;
- R4: `- x=$(void_f)` escapes SB4076;
- R5: a compound `-` still covers its own direct commands. This is settled
  by `retire-command-groups` and `retire-suppression-on-compounds`, and
  listed only so its rows have a home.

Nothing in this unit changes them, per DG 2026-10-28: no code that cements
the existing suppression story.

## Decide-and-report (DG may veto)

- The reach list, as the DD55 addendum gives it.
- A failure in a function stage of a capture condition, or in a stage's
  operand capture there, aborts only the stage child. The condition sees a
  failed stage and is false (noreach.6, .13). That is F2's stage-child
  model, not a new rule.
- A builtin's operand captures are covered like an external's (noreach.8).
- A bare `$( )` inside arithmetic is SB4035, so the arithmetic barrier is
  pinned through a parameter default, `${m[$(cmd)]:-0}` (noreach.14).
- Five unsuppressed controls behave the same on both binaries (qd/qe: a
  suppressed and a plain `(( n = 1 / z ))`; qn/qo/qp: an index capture in
  an arithmetic store, a default and `$(( ))`).

## A loose end: e18

One probe-results file recorded SB4016 for
`fn f { /bin/false; echo tok; return 1 }; str x=$(- f)`, a program that
noreach.5 compiles. The probe file was edited at 20:19, a minute after the
results file was written at 20:18, so that verdict belonged to the earlier
text. Re-run with absolute binary paths, under default flags and
`--no-taint`, the pre-unit binary prints `[tok]` and the unit B binary
aborts at the `/bin/false`, as noreach.5 says.

## Notes

DD55: the 2026-10-20 "Suppression scopes the complete call" section is
replaced by "A `-` discards one status", the `- myfunc` note gains a
sentence, and the unit B addendum is new. DD70: the `- f` line is marked
ruled. GUIDE: the suppression paragraph describes the depth.
`q-suppression-on-user-functions`: Q1 and Q4 landed, and the task closes at
the F6 wrap-up. Probes: files/port-f6-probes.tar.gz (`f6b/`), refreshed at
the wrap-up.

Next: unit C (B84, 55.88), then the F6 wrap-up.
