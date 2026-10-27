---
title: "Question: the residual `-` forms, `exec` in a forked child, a here-string on a compound, and a value intrinsic as a stage"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on each question below; each ruling is recorded in the DD it names"
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

### Q1. `-` on a control word is always inert

| Id | Program | Result |
|---|---|---|
| dr1 | `fn f { - return 3 }` then `f` then `echo "[$?]"` | `[3]` |
| dr2 | `- exit 3` then `echo after` | exits 3, prints nothing |
| dr3 | `for x in a b {` then `- break` then `echo "in $x"` then `}` then `echo after` | `after` |
| dr4 | dr3 with `- continue` | `after` |
| dr5 | `- :` then `echo after` | `after` |
| dr6 | `fn f { - return /bin/false }` then `f` then `echo "[$?]"` | `[1]` |
| r6 | dr6 without the `-` | `[1]` |

`return` and `exit` pass their status on, and `break`, `continue` and `:`
cannot fail, so the `-` has nothing to discard. In dr6 DD#74 already makes
`return` the handler of its producer. A failing capture argument does not
give the `-` a job either:

| Id | Program | Result |
|---|---|---|
| dr10 | `fn g { echo 3; return 1 }` then `fn f { - return $(g) }` then `f` then `echo "[$?]"` | `[3]` |
| dr11 | dr10 without the `-` | `[3]` |
| dr12 | `fn g { echo 3; return 1 }` then `- exit $(g)` | SB4030 "`exit` takes an exit status, but a command substitution is string-typed …" |
| dr13 | dr12 without the `-` | SB4030, the same |
| dr14 | `fn f { - return $(/bin/sh -c 'echo 3; exit 1') }` then `f` then `echo "[$?]"` | SB5020 |
| dr15 | dr14 without the `-` | SB5020 |

dr10 and dr11 compile only because `return` coerces a string operand, which
is `bug-return-status-string-operand`. With that fixed they are SB4030 like
dr12 and dr13, and the question stands as it is.

Nor does the `-` discard an argument's own abort. `return ${s:?empty}` and
`exit ${s:?empty}` (with `str s=""`), and `return $(( 1 / z ))` and
`exit $(( 1 / z ))` (with `int z=0`), abort the same with and without it
(`t6b/` q1a–q1f, q1j). `:` aborts on neither today, because it drops every
argument effect except a capture's; that is `bug-noop-drops-argument-effects`,
and with it fixed the `-` on `:` still discards nothing. `: $(g)` and
`- : $(g)`, where g echoes and returns 1, both print `after` (q1k, q1g): a
function capture's own `return` status is unchecked in every position
(the batch-531 B15 family).

- **(a) Recommended.** Add `return`, `exit`, `break`, `continue` and `:` to
  `unsuppressible_error`: SB4014 at the `-`, "drop the `-`", beside `fn`
  and `END`. It is the principle F1+F4 applied to those: a `-` that
  discards nothing is a choice the reader should not have to evaluate.
- (b) Keep them legal and inert, and say so in DD#55.

### Q2. `exec` in a forked child: rejected as a stage, legal under `&`

| Id | Program | `8e28c61` | F1+F4 |
|---|---|---|---|
| ex3 | `exec /bin/true &` then `- wait` then `echo after` | `after` | `after` |
| ex2 | `/bin/echo a \| exec /bin/cat` | rc 141, "sash: bug: bare command passed to sash_exec (expected absolute path)" | SB4022 1:15 "`exec` isn't allowed as a pipeline stage — a stage already runs in a forked child, so `exec` there replaces or rewires that child, not the program; write the command without `exec`, or run `exec` as its own statement" |
| ex4 | `exec 3>FILE &` then `- wait` then `echo after` | `after` | SB4024 1:1 "`exec` without a command isn't allowed in a background job — the job (`… &`) runs in a forked child, so its effect would land on the child and be lost; drop the `&`" |
| ex1 | `exit 3 &` then `- wait` then `echo after` | `after` | SB4024 1:1, the same reason |

ex3 and ex2 both run `exec` in a forked child, where it replaces only that
child, so each equals the command without `exec`. F1+F4 rejected the stage
(batch-531 B12: it crashed) and kept ex3 legal on purpose, because ex3
works and its fate is a language question rather than a bug.

- **(a) Recommended.** Reject `exec cmd &` with SB4024, "write the command
  without `exec`". It equals `cmd &`, and SB4022's reason for the stage
  applies word for word.
- (b) Accept `a | exec b` as `a | b`, matching ex3.
- (c) Keep the split, and record why in DD#55.

### Q3. `-` where the status is consumed

DD#55 (lines 26–29): "`- cmd` … its exit status is discarded. `$?` is not
available after `- cmd` — reading `$?` after `-` is a compile error. You
said you don't care; you don't get to inspect it." Its 2026-07-24 section:
a pipeline's status is the worst over its *non-suppressed* segments, and a
`-` on a segment excludes it.

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

e04, dc2, e07 and v9 read the status DD#55 says was discarded. e03 applies
the aggregate rule to zero segments, which gives success, so it disagrees
with e04 about the same `- /bin/false`. e08–e10 follow DD#55. A stage `-`
that excludes a SIGPIPE-prone producer is meaningful (DD#55, lines 94–97)
and stays legal under every option.

- **(a) Recommended.** Reject a `-` whose discarded status is the one
  being read: a lone `- cmd` as an `if`/`while`/`until` condition, a `-`
  on the left operand of `&&`/`||`, a `-` on a capture condition, and a
  condition pipeline whose every segment is suppressed. A final `a || - b`
  at statement level stays legal, since nothing reads that status. This
  is DD#55's "reading `$?` after `-` is a compile error", applied to the
  other places a status is read.
- (b) Define a discarded status as success everywhere, so `if - cmd` is
  always true, as e03 already is.
- (c) Document the split in DD#55.

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
