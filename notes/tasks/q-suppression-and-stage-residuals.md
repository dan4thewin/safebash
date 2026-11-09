---
title: "Question: the residual `-` forms, `exec` in a forked child, a here-string on a compound, and a value intrinsic as a stage"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the remaining questions below; Q1, Q2, Q3 and Q5 are settled in DD31/DD55/DD70, with separate implementation tasks"
  - "every program in the tables prints what its ruling says or is rejected with a diagnostic naming the rule, in both backends"
  - "SB4018's `[[ … ]]` advice and SB5073's message for a stage say only what the rulings on Q5 and Q6 allow"
  - "Q5 enforcement and advice belong to reject-pointless-statements; bug-peel-family-classifiers retains its shared-classifier repairs"
---

## What

Found 2026-10-27, while porting F1+F4 (journal 537). F1+F4 settled where
`-` and `!` may appear and which state effects may run as a stage or under
`&` (the 2026-10-26/27 addenda to DD#55, DD#70, DD#71 and DD#122). Its
`unsuppressible_error` (sash/src/parse/parser.rs) makes `- !` SB4019, and a
`-` before `attest`, a declaration, `fn` or `END` SB4014 at the `-`. These
are the cases it left alone. Results are the same at `8e28c61` (before
F1+F4) and with F1+F4, in both backends, unless a row says otherwise.

Probes: `files/port-f1f4-probes.tar.gz`, directories `dr/`, `bk/`, `bl/`,
`rw/`, `adv/`, `fin2/`, `t6/` and `t6b/`.

### Q1. Settled: control/arithmetic prefixes rejected; intentional no-op retained

DG chose **reject** on 2026-11-08 for source `-` on `return`, `exit`,
`fail`, `break` and `continue`. DD55 records the rule; the new P3
`reject-control-transfer-suppression` owns enforcement and journal 591
records the choice. This cohort is settled as policy, not implemented.
DD70's subsequent statement-effect ruling retains the intentional no-op.
DD31 then settled arithmetic mutations as no-result writes and rejected
arithmetic prefixes (journal 593). Q1 is settled as policy; the
implementation owners remain open.

The tables retain historical F1+F4 evidence, not fresh execution or a
claim that later return-kind and capture rules leave every fixture legal.
The rejected cohort includes DD128's newer `fail`, which those old
probes did not exercise. A reached `exit` or `fail`
terminates rather than returning a catchable status; `-` cannot undo
that transfer.

| Id | Program | Result |
|---|---|---|
| dr1 | `fn f { - return 3 }` then `f` then `echo "[$?]"` | `[3]` |
| dr2 | `- exit 3` then `echo after` | exits 3, prints nothing |
| dr3 | `for x in a b {` then `- break` then `echo "in $x"` then `}` then `echo after` | `after` |
| dr4 | dr3 with `- continue` | `after` |
| dr5 | `- :` then `echo after` | `after` |
| q1h | `str s=""` then `- : ${s:?empty}` then `echo after` | aborts with `empty` since 33616b6 (`:` evaluates its arguments, journal 539); printed `after` before. `-` does not discard a disposition abort |
| dr6 | `fn f { - return /bin/false }` then `f` then `echo "[$?]"` | `[1]` |
| r6 | dr6 without the `-` | `[1]` |

The historical control-transfer rows left the transfer unchanged. `return` and
`exit` pass their status on, while `break` and `continue` transfer within
the loop. In dr6 DD74 already makes `return` the handler of its producer.
A failing capture argument does not give the prefix reach into that
argument:

| Id | Program | Result |
|---|---|---|
| dr10 | `fn g { echo 3; return 1 }` then `fn f { - return $(g) }` then `f` then `echo "[$?]"` | `[3]` |
| dr11 | dr10 without the `-` | `[3]` |
| dr12 | `fn g { echo 3; return 1 }` then `- exit $(g)` | SB4030 "`exit` takes an exit status, but a command substitution is string-typed …" |
| dr13 | dr12 without the `-` | SB4030, the same |
| dr14 | `fn f { - return $(/bin/sh -c 'echo 3; exit 1') }` then `f` then `echo "[$?]"` | SB5020 |
| dr15 | dr14 without the `-` | SB5020 |

The historical dr10/dr11 string coercion belongs to
`bug-return-status-string-operand`. It is not a reason to preserve the
prefix. Later capture/return-kind rules must also be considered before
reusing these fixtures.

Nor does the `-` discard an argument's own abort. `return ${s:?empty}` and
`exit ${s:?empty}` (with `str s=""`), and `return $(( 1 / z ))` and
`exit $(( 1 / z ))` (with `int z=0`), abort the same with and without it
(`t6b/` q1a–q1f, q1j). The old claim that `:` skipped these effects was
repaired in journal 539 (`bug-noop-drops-argument-effects`, DD69):
its arguments are evaluated, and a prefix does not suppress their
failures. The q1k/q1g captured-status behavior is historical B15 evidence,
not permission to capture a status function without the handling/discard
now required by DD55.

**Required:** reject the source prefix with a located "drop the `-`"
explanation. Bare/valued returns, return producers and forwarding,
default/explicit termination statuses and counted loop transfers all
follow the rule. Wrappers, attached redirects and body placement do not
grant an exception. Independent typing and placement refusals remain.
Non-void call-site discard eligibility, return handling, constant results,
no-reach and terminal routing are unchanged. Keeping the redundant
control-transfer spellings was the alternative, not the chosen rule.

**The intentional no-op is retained (DG, 2026-11-08; DD70).** DD69 treats
`:` as status-producing (`! :` supplies the opposite verdict), and its
arguments can have effects.
Under DD130, `- : >optional.log` can discard a recoverable open failure;
calling every `- :` inert is therefore wrong. The statement-effect rule
also preserves explicit `:` without effects, including an otherwise
legal `- :`. It is a permitted no-op, not an effect that excuses a
pointless enclosing test. Q5 and journal 592 record that distinction.

**Arithmetic prefixes are rejected (DG, 2026-11-08; DD31).** Assignments
and mutations have no result, so `- (( n += 1 ))` has nothing to discard.
Comparisons produce results and cannot be arithmetic statements, even
with `-`; bare `(( 0 ))`/`(( n + 1 ))` are not status-producing statements
either. Parentheses do not give a write a result, and a write cannot
supply another calculation's operand value.
`q-arith-test-statement-status` now owns arithmetic enforcement and fixture
migration. R2's operand barrier retains its independent owner in
`reject-nonadjacent-suppression`; a prefix refusal is not proof of that
barrier. DD70's discard/effect rule for other operations remains intact.

### Q2. Settled: direct external jobs; reject the redundant `exec` prefix

DG, 2026-11-08: **"let's drop the wrapper - the simplification for
signalling outweighs the diagnostic"**. DD55 now requires a job of one
external command to exec directly in the background child. `$!` names
the command, not a sash wrapper. The parent can still reap its outcome
through DD126's `wait` / `waitstatus`; the automatic completion
diagnostic goes, without a new parent-side reporter.

A backgrounded user function remains an explicit checked-wrapper option,
including when its body contains only one external command. Functions,
builtins, pipelines and multi-command lists retain their existing
process/body semantics. Command stderr and exec-startup errno diagnostics
are not silenced. Fork failure remains fatal under DD130.

The first recording chose former option (d)'s direct-process behavior
without inferring its additional spelling ban. Asked separately on
2026-11-09, DG chose **reject** for a direct single-command `exec cmd &`
job. Advise `cmd &`: dropping `&` would turn it into foreground process
replacement. Arguments, redirects and additional marks do not make
the direct-job prefix useful.

This is not a blanket ban on exec in forked code. An otherwise legal
exec inside a user function or multi-command body remains meaningful.
Conversely, placing the redundant direct job inside a function does
not exempt it. Ordinary foreground exec, the existing fd-only rules
and the pipeline-stage refusal remain unchanged. An otherwise legal
`- cmd &` still uses the direct child.

Existing P3 `background-external-direct-exec` owns both changes and
fixture migration. The refusal must ship with the direct-exec default,
not remove the current direct-PID route before its replacement exists.
Journals 594 and 595 record the rulings. Q2 is settled as policy, not
implemented. The tables below retain historical evidence, not fresh
measurements or gate predictions.

#### Historical evidence

| Id | Program | `8e28c61` | F1+F4 |
|---|---|---|---|
| ex3 | `exec /bin/true &` then `- wait` then `echo after` | `after` | `after` |
| ex2 | `/bin/echo a \| exec /bin/cat` | rc 141, "sash: bug: bare command passed to sash_exec (expected absolute path)" | SB4022 1:15 "`exec` isn't allowed as a pipeline stage — a stage already runs in a forked child, so `exec` there replaces or rewires that child, not the program; write the command without `exec`, or run `exec` as its own statement" |
| ex4 | `exec 3>FILE &` then `- wait` then `echo after` | `after` | SB4024 1:1 "`exec` without a command isn't allowed in a background job — the job (`… &`) runs in a forked child, so its effect would land on the child and be lost; drop the `&`" |
| ex1 | `exit 3 &` then `- wait` then `echo after` | `after` | SB4024 1:1, the same reason |

F1+F4 rejected the stage (batch-531 B12: it crashed) and kept ex3 legal on
purpose, because ex3 worked and its fate was a language question rather
than a bug.

Corrected 2026-10-27. This question first said that ex3 and ex2 each equal
the command without `exec`, and recommended rejecting ex3. That holds for the
stage, not for the job. Measured at `d60e74f`, both backends unless a row says
`strace`:

| Id | Program | Result |
|---|---|---|
| p1 | `/bin/echo a \| /bin/cat`, under `strace -f` | 2 forks; each stage's child execs its command |
| s1 | `/bin/true &` then `- wait`, under `strace -f` | 2 forks: a sash child, which forks and execs `/bin/true` |
| s3 | `- /bin/true &`, the same | 2 forks, as s1 |
| s2 | `exec /bin/true &`, the same | 1 fork: the child execs `/bin/true` |
| e4 | `/bin/sleep 1 &` then `/bin/cat /proc/$!/comm` | the program's own name: `$!` is the sash child |
| e5 | `exec /bin/sleep 1 &`, the same | `sleep`: `$!` is the command |
| k1 | `/bin/sleep 7.31 &`, `j=$!`, `/bin/kill $j`, `- wait`, then `pgrep` for the sleep | still running, orphaned |
| k3 | `- /bin/sleep 7.33 &`, the same | still running, orphaned |
| k2 | `exec /bin/sleep 7.32 &`, the same | gone |
| w1 | `/bin/false &`, `- wait`, then `int s=$(waitstatus)` and print it | `st=1`; the child prints "/bin/false: command failed (exit 1)" |
| w3 | `- /bin/false &`, the same | `st=1`, nothing else |
| w2 | `exec /bin/false &`, the same | `st=1`, nothing else |
| fg1 | `exec /bin/false` then `echo after` | rc 1, nothing printed |

In those measurements, a pipeline stage already exec'd directly while an
ordinary background job kept a wrapper. `exec cmd &` supplied direct PID
ownership, and `- cmd &` kept the wrapper without its completion report.
All three exposed the outcome through `waitstatus`.

The AI initially recommended retaining that split. DG asked what the
wrapper actually provided, confirmed that status collection did not need
it, and identified a backgrounded user function as the explicit
replacement when checking was wanted. DG then chose to remove the
implicit wrapper. Neither a parent-side reporting mechanism nor allowing
redundant pipeline-stage `exec` was selected.

Probes: directory `exq/` in `files/port-exec-probes.tar.gz`.

### Q3. Settled: reject consuming a discarded status

DD55's 2026-11-08 adjacency ruling settles the prefix's recipient:
`- a || b` prefixes only `a`, not the whole list or `b`.
`reject-nonadjacent-suppression` owns that enforcement. DG then chose
**reject** for consuming the discarded result. DD55/DD70/DD74/DD88 record
the rule; `reject-discarded-status-consumers` owns enforcement and journal
589 records the choice. This question is closed, not the implementation;
the other questions in this file remain open.

DD#55, "Suppression with `-`": "`- cmd` … its exit status is discarded.
`$?` is not available after `- cmd` — reading `$?` after `-` is a compile
error. You said you don't care; you don't get to inspect it." Its
2026-07-24 section:
a pipeline's status is the worst over its *non-suppressed* segments, and a
`-` on a segment excludes it.

The following are historical F1+F4 measurements, not fresh execution or
claims that later typing/return rules leave every fixture legal.

| Id | Program | Result |
|---|---|---|
| e04 (= dc1) | `if - /bin/false { echo t } else { echo f }` | `f` |
| dc2 | `if - /bin/true { echo t } else { echo f }` | `t` |
| e07 | `- /bin/false \|\| echo caught; echo after` | `caught`, `after` |
| v9 | `if x=$(- /bin/false) { echo "y${x:!}" } else { echo n }` | `n` |
| e03 | `if - /bin/false \| - /bin/false { echo t } else { echo f }` | `t` |
| e08 | `if /bin/false \| - /bin/true { echo t } else { echo f }` | `f` |
| e09 | `if - /bin/true \| /bin/false { echo t } else { echo f }` | `f` |
| e10 | `- /bin/false \| /bin/true` then `echo "[$?]"` | `[0]` |

e04, dc2, e07 and v9 read the discarded status. e03 instead formed a
successful aggregate over zero unsuppressed stages. All five shapes now
require refusal. The partial-pipeline principle in e08/e09 remains:
the condition consumes only unsuppressed stages. Status availability in
other positions retains DD55's separate rules; e10's historical result is
not a new permission to read `$?` after an ordinary statement.

Reject a directly suppressed result used by `if`/`elif`/`while`/`until`,
an `&&`/`||` decision, a capture condition or a status return producer.
DD74's existing context rule includes middle clauses and the final clause
of a consumed list: `a || - b` is legal as a statement, but
`if a || - b { ... }` and `return a || - b` are not. An all-suppressed
pipeline cannot supply a consumed status; partial pipelines retain their
aggregate and statement-level all-suppressed pipelines remain legal.

The rule is about the result being consumed, not the mere presence of
`-` inside a condition or function. Captured stdout may be used as a text
operand, subject to taint, while its command's status is discarded.
Likewise `- cmd; return 0` supplies a fresh status rather than forwarding
the discarded one. Ordinary condition/return handling may disable an
implicit abort internally without being source-level discard.

The alternatives of making discard mean success or preserving the
single-command/all-suppressed-pipeline split were not selected.

### Q4. A here-string on a compound reports the redirect, not the here-string

| Id | Program | Result |
|---|---|---|
| hs3 | `/bin/cat <<< x` | SB4050 1:1 "sash has no here-strings (`<<<`) — they are exactly a pipe in sash …" |
| hs2 | `{ /bin/true } <<< x` | SB4021 1:1 "a redirect on a compound command isn't allowed — wrap the compound in a function" |
| hs1 | `! { /bin/true } <<< x` | SB4050, at 1:3 with F1+F4 and 1:1 at `8e28c61` |

hs2's advice leads to a function whose here-string is then SB4050: DD#83
rejects `<<<` "on any operand". SB4050 also points at the statement start,
never at the `<<<`.

- **(a) Recommended.** SB4050 wins wherever `<<<` appears, and points at
  the `<<<`.
- (b) Keep the order as it is.

### Q5. Settled: legal discard, separate pointless-statement error

DG proposed keeping discard allowed while rejecting a pointless
statement, then approved the conservative effect-based rule on
2026-11-08. DD70 records it; `reject-pointless-statements` owns
enforcement, and journal 592 records the decision. The question is
settled, not the implementation.

Discarding a real boolean remains legal as an operation. Reject the
whole statement when its only work is computing an ignored result and
the absence of other observable effects is known. The rows below are
historical measurements, not fresh execution.

| Id | Program | Result |
|---|---|---|
| k01 | `s=abc` then `- [[ $s =~ "b" ]]` then `echo after` | `after` |
| pk3 | `str s=abc` then `- match s ^a` then `echo "$s"` | `abc` |
| k02 | `s=abc` then `[[ $s =~ "z" ]]` then `echo after` | SB4018 2:1 "`[[ … ]]` result is unused — consume it with if/`&&`/`\|\|`, or discard with `-` or `\|\| :`" |
| pk4 | `- (( 1 > 2 ))` then `echo after` | SB5007 1:3 "comparison 1 > 2 as bare statement has no effect — use in if/while condition" |
| pk5 | pk4 without the `-` | SB5007 1:1, the same message |

A `[[ … ]]`, and a `match` with no out array, produce nothing but a status.
DD#63 binds only the out-array form, `match SUBJECT PATTERN OUTARRAY`
(spec/regex.sh: `match "abc" "b" g`); "the boolean form still binds
nothing", and `[[ =~ ]]` is that form's sugar. In pk3 the subject is the
literal word `s`, so the row shows only that nothing is bound. With
effect-free operands and no redirects, k01 and pk3 require the new
pointless-statement refusal. Statement-level `[[ $s == yes ]] || :`
requires it too: the intermediate branch does not give unused computation
a purpose. Choosing observable work or supplying a return/value is
different; a condition selecting only no-ops is not an escape.

Writes, I/O and observable operand/body effects prevent rejection, as
does uncertainty. Out-array `match`/`matchall`, effectful calls and
`- : >optional.log` retain their work even when their result is discarded.
Plain `:` remains an intentional no-op, not a way to make an enclosing
result-only test useful. Existing result-kind and placement rules remain.

The preceding question framing overstated dynamic-regex failure as an
effect: when that recoverable failure is itself discarded, DD70 requires
silence and continuation. It does not rescue the statement. A checked
operand failure remains observable under no-reach. Incidental allocation
or possible OOM also does not justify pointless computation; fatal faults
in retained programs remain fatal.

This is not a constant-result ban. A constant-returning function and
calls that use its result remain legal; a known effect-free `- f` can
still be a pointless invocation. Uncertain effects must not be assumed
away. No general dead-store or whole-program usefulness proof is required.

Neither the old blanket test-discard ban nor unconditional permission
for every discarded test was selected. Reconcile SB4018's advice with
the two independent checks: `-` and `|| :` cannot be advertised as
unconditional escapes for a pointless test. The shared-classifier work
in `bug-peel-family-classifiers` remains required on effectful forms.

### Q6. A value intrinsic as a pipeline stage

| Id | Program | Result |
|---|---|---|
| st1 | `str s=a` then `/bin/echo a \| strlen $s` | SB5073 2:15 "`strlen` is a value intrinsic — it produces a value and is only valid inside a command substitution, e.g. `x=$(strlen …)`. A bare statement would discard the value" |
| st2 | `/bin/echo a \| getenv HOME` | SB5073, the same |
| st3 | `str s=a` then `strlen $s \| /bin/cat` | SB5073 2:1, the same |
| st4 | `getenv HOME \| /bin/cat` | SB5073, the same |
| st5 | `str s=a` then `strlen $s` | SB5073 2:1, the same |

The rejection is DD#111's. Only st5 is the "bare statement" the message
describes, and in st3 and st4 the value would not be discarded: it would
be the stage's output.

- **(a) Recommended.** Keep SB5073 and give a stage its own wording.
- (b) Let a value intrinsic stage write its value to the pipe. That needs
  a taint rule first: DD#24's 2026-05-24 clarification decides a reader's
  taint by whether the pipe's source execs, and a `getenv` stage does not,
  so `getenv HOME | readline x` would read clean although DD#24 counts an
  environment read as a taint source.
