---
title: "Question: the residual `-` forms, `exec` in a forked child, a here-string on a compound, and a value intrinsic as a stage"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the remaining questions below; Q3 is settled in DD55 with enforcement in reject-discarded-status-consumers"
  - "every program in the tables prints what its ruling says or is rejected with a diagnostic naming the rule, in both backends"
  - "SB4018's `[[ … ]]` advice and SB5073's message for a stage say only what the rulings on Q5 and Q6 allow"
  - "`bug-peel-family-classifiers`, which defers a case to Q5, is updated to the ruling"
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

### Q1. Control-transfer prefixes; the no-op is a separate case

The tables retain historical F1+F4 evidence, not fresh execution or a
claim that later return-kind and capture rules leave every fixture legal.
The remaining control-transfer choice covers `return`, `exit`, `break`
and `continue`, and now `fail` under DD128. A reached `exit` or `fail`
terminates rather than returning a catchable status; `-` cannot undo
that transfer. No prefix-acceptance ruling has been made for this cohort.

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

The control-transfer rows leave the transfer unchanged. `return` and
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

- **(a) Recommended for control transfers.** Reject a prefix on `return`,
  `exit`, `fail`, `break` or `continue`, with a located "drop the `-`"
  diagnostic. The prefix neither cancels the transfer nor handles
  failures inside an operand.
- (b) Keep those control-transfer spellings legal and inert, and document
  that they do not suppress the transfer.

**The no-op is a separate case.** DD69 treats `:` as status-producing
(`! :` supplies the opposite verdict), and its arguments have effects.
Under DD130, `- : >optional.log` can discard a recoverable open failure;
calling every `- :` inert is therefore wrong. Any proposal to reject
the bare redundant form must distinguish this real file operation,
not silently remove its handling. No new policy on `:` is selected here.
The eligibility of a direct arithmetic-write prefix, `- (( ... ))`,
also remains separate (R2 in `q-suppression-reach-residuals`); its required
operand-barrier repair does not decide that question.

### Q2. `exec` in a forked child: rejected as a stage, legal under `&`

| Id | Program | `8e28c61` | F1+F4 |
|---|---|---|---|
| ex3 | `exec /bin/true &` then `- wait` then `echo after` | `after` | `after` |
| ex2 | `/bin/echo a \| exec /bin/cat` | rc 141, "sash: bug: bare command passed to sash_exec (expected absolute path)" | SB4022 1:15 "`exec` isn't allowed as a pipeline stage — a stage already runs in a forked child, so `exec` there replaces or rewires that child, not the program; write the command without `exec`, or run `exec` as its own statement" |
| ex4 | `exec 3>FILE &` then `- wait` then `echo after` | `after` | SB4024 1:1 "`exec` without a command isn't allowed in a background job — the job (`… &`) runs in a forked child, so its effect would land on the child and be lost; drop the `&`" |
| ex1 | `exit 3 &` then `- wait` then `echo after` | `after` | SB4024 1:1, the same reason |

F1+F4 rejected the stage (batch-531 B12: it crashed) and kept ex3 legal on
purpose, because ex3 works and its fate is a language question rather than a
bug.

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

In a stage `exec` changes nothing: the stage's child already becomes its
command, and the parent checks its status. Under `&` it changes what the job
is. A plain job is a sash child that forks the command, checks it and reports
a failure. `$!` names that child, so a signal sent to `$!` stops the child and
leaves the command running. `exec cmd &` is the only spelling whose job is the
command itself: one process, named by `$!`, reached by a signal. Like a
foreground `exec`, it leaves nothing behind to report a failure, so it reports
as `- cmd &` does. The status still reaches `$(waitstatus)`.

- **(c) Recommended.** Keep the split, and record the reasons above in DD#55.
  `exec` as a stage is a spelling that does nothing, and SB4022 says to drop
  it. `exec cmd &` is how a job is made signallable, and its unreported
  failure is what `exec` means.
- (a) Reject `exec cmd &` with SB4024. Then no job can be signalled through
  `$!` without orphaning its command, unless (d) is chosen too.
- (b) Accept `a | exec b` as `a | b`: a second spelling of the same stage.
- (d) Run a job of one external command as a stage runs: its child execs the
  command. `$!` then names the command and `exec cmd &` is redundant, so (a)
  follows. But an unmarked job's failure is then reported by nobody; only
  `wait` and `$(waitstatus)` see it, unless the parent reports what it reaps.

Whichever option: is it acceptable that a signal sent to a plain job's `$!`
stops only the sash child and orphans the command (k1, k3)? Under (c) the
answer is to write `exec cmd &` when the job must be signalled. Only (d)
changes it for `cmd &`.

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

### Q5. A `-` that leaves a statement with no effect

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
literal word `s`, so the row shows only that nothing is bound. Discard the
status and the statement does nothing, which
is SB5007's case for `(( 1 > 2 ))` with or without the `-`. SB4018's advice
recommends k01's spelling, and its other suggestion, `[[ … ]] || :`, is
just as empty. An operand capture is the one effect such a statement can
still have; whether that counts is part of the question.

- **(a) Recommended.** Reject `- [[ … ]]` and a `- match` with no out
  array as having no effect, as SB5007 does, and drop the "discard with
  `-` or `|| :`" half of SB4018's `[[ … ]]` advice.
- (b) Keep them legal, and say in DD#70 that a discarded test is a no-op.

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
