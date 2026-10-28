---
title: "Question: what `-` may discard on a user-function call, and how far it reaches"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG rules on each question below; each ruling is recorded in the DD it names"
  - "every program in the tables prints what its ruling says or is rejected with a diagnostic naming the rule, in both backends"
  - "SB4016's advice and the texts of DD#55 and DD#70 say only what the rulings allow"
  - "`bug-peel-family-classifiers`, `bug-fork-failure-reporting-by-form` and `dd55-nonvoid-functions` are updated to the rulings"
---

## What

Found 2026-10-28, while porting F2 (batch-531 B82: a stage's `-` is not
threaded into a function stage's child). DG, in three messages that day:

> re B82, I don't think suppression extends into the user function -
> that's the same problem as using a user function as a condition, yes? re
> B81, I think when it's entirely known that a segment never writes to
> stdout, it probably should be an error for it to be the LHS of a pipe,
> and probably vis versa for read/RHS. cmd "$(g)" | cat should be a taint
> error unless g is known to be clean...

> I'm beginning to think supression shouldn't be allowed for user
> functions, since the user controls teh return value and can suppress
> within the function... discuss

> I think the point of suppression is to say I-don't-care about an
> external program or maybe a system call result, to make a better
> alternative to the || : bash spelling. We have probably over generalized
> it. A user function, even in a child process, is entirely under the
> programmer's control (save where it runs externals which it may
> suppress). Take all the potential changes from this and put them into
> the backlog as requiring further consideration. Avoid making code changes
> now that would further cement the existing suppression story.

Every change below requires further consideration. Until DG rules, no
code, spec or advice may entrench `-` on a user-function call. F2 lands
without the B82 wrap, and without specs that pin `- f |` or `- f &`.

The governing texts pull in different directions:

- DD#70's distinguishing principle (:79–85) is DG's reading today: `-` is
  "for **opaque externals**", while "**in-process logic you control**",
  including "your own `-> int` function", is restructured rather than
  suppressed. Its 2026-09-09 addendum (:87–97) superseded the principle:
  "`- X` now uniformly means **"run X, discard its result"**", and `- f` for
  an `-> int` function is named legal. The 2026-09-24 addendum (:332–345)
  made `- f` the replacement for `f || :`, which became SB4030 for an
  `-> int` function.
- DD#55's non-void rule (:217–222) lists "`- myfunc` — suppressed" as one
  way to handle a non-void call.
- DD#55's "Suppression scopes the complete call" (:67–81, 2026-10-20)
  extends the prefix to "the called body, including commands in branched
  bodies". The AI wrote that text as a decide-and-report in coverage 530
  (`1309385`); DG never ruled on it.
- Against any reach into a body: DD#86 :455–458 (the condition context
  resets on entering a body, "exactly as they stop at `fn`"), DD#88 :42–45
  (a function capture is never a capture condition: "functions convey
  status via `return`") and DD#74 :44–48 (a predicate's status comes
  "**only** from an explicit `return`").

Results are the same at `899179d` and on F2's trial tree (not landed), in
both backends, unless a row says otherwise. "Aborts" means rc 1, stderr
`/bin/false: command failed (exit 1)` ("the message"), and nothing printed
after it. ND is `/nonexistent/d/x`; its failed open prints
`/nonexistent/d/x: No such file or directory` ("ND's message"), and
`missing` is a relative name that does not exist. A failing function
stage is named `__compound_N` at `899179d` (batch-531 B10); F2 names the
function, and the rows quote F2's name. Rows marked `--no-taint` were
compiled with that flag.

Probes: `files/port-f2-supfn-probes.tar.gz`. `qx/manifest.txt` maps each
id to its program, its compiler flags and its fault injection, and
`qx/fresh.tsv` holds the 2026-10-28 results on both trees in both
backends. Rows the manifest lacks are in `supfn/xw-results.txt` (x07–x17,
w23), `supfn/gp-results.txt` (p01, p02, p06–p09), `supfn/r4/results.txt`
(r4/r04–r08), `supfn/q5b/results.txt` (b1–b3) and `supfn/l/results.txt`
(l1–l4, also run at `1309385` and on batch-531's branch, `e98fca8`).
Every p row's program is in `supfn/p/`; the top-level `supfn/p*.sh` are
an earlier round. Spec
censuses: `supfn/census/c3.py` (`-` on a call), `c4.py` (a capture of a
status function), `c2.py` (ceremony returns and bare calls of a status
function; its output is `f2/census-out.txt`) and `c5.py` (a status
function as an `&&`/`||` operand; `c5ctl.py` is its control).

### Q1. `-` on a call reaches into the body, except on a stage

| Id | Program | Result |
|---|---|---|
| c01 | `fn f { /bin/false; echo x; return 1 }` then `if f { echo t } else { echo e }` then `echo done` | aborts |
| c02 | c01's f, then `f \|\| echo h` then `echo done` | aborts |
| c03 | c01's f, then `- f` then `echo done` | `x`, `done` |
| v02 | `fn f { /bin/false; echo x }` then `- f` then `echo done` | `x`, `done` |
| t05 | `fn f -> int { /bin/false; return 7 }` then `- f` then `echo done` | `done` |
| p03 | v02's f, then `- { f }` then `echo done` | `x`, `done` |
| g5 | v02's f, then `- { /bin/true; f }` then `echo done` | `x`, `done` |
| x03 | `fn f { /bin/false; echo inner }` then `f &` then `- wait` then `echo after` | `after`, and the child's message |
| x04 | x03 with `- f &` | `inner`, `after` |
| x14 | `fn f { echo x; /bin/false; echo y }` then `f &` then `- wait` then `int s=$(waitstatus)` then `echo "s=$s"` | `x`, `s=1`, and the child's message |
| x15 | x14 with `- f &` | `x`, `y`, `s=0`, silent |
| v01 | v02's f, then `- f \| /bin/cat` then `echo done` | `done`, and the child's message; no `x`, rc 0 |

c01 and c02 confirm DG's analogy: a condition does not reach into f, so
f's `/bin/false` aborts. A `-` on the statement (c03, v02, t05), on a
group around the call (p03, g5) or on a job (x04, x15) does reach: the
`/bin/false` goes unchecked and the body runs on. A `-` on a stage does
not (v01): the child aborts, and the `-` discards only the stage's status.
That asymmetry is batch-531 B82, which F2 was to fix by threading the `-`
into the child (the "B82 wrap").

A `-` on a group or an `if` reaches every command inside it, while `||`
cannot take a group at all:

| Id | Program | Result |
|---|---|---|
| g1 | `- { /bin/false; echo x }` then `echo done` | `x`, `done` |
| g2 | `- if /bin/true { /bin/false; echo x }` then `echo done` | `x`, `done` |
| g3 | `{ /bin/false; echo x } \|\| echo h` | SB4023 1:1 |
| p11 | v02's f, then `{ f } \|\| :` | SB4023 2:1 |

So Q1 stands even if Q2 rejects `- f`: `- { f }` (p03) still reaches f's
body through the group.

- **(a) Recommended.** No reach. A `-` on a call discards the call's
  status, and everything inside the body is checked as usual, as it is
  under a condition (c01, c02) and as DD#86 says boolean contexts stop at
  `fn`. A command in the body whose failure does not matter takes its own
  `-` there. Correct DD#55 :67–81. Flips supval.1–.5, .9, .12 and .22;
  supval.11 needs a new program.
- (b) Keep the reach, and extend it to the stage (the B82 wrap: v01 would
  print `x`). Record in DD#55 that `-` reaches where a condition does not.

### Q2. May `-` prefix a user-function call at all?

With the reach removed (Q1), this is what `-` discards on each form:

| Form | `-` discards | Rows |
|---|---|---|
| `- f`, f void | nothing | w04 |
| `- f`, f returns a status | the authored `return`; it differs from a bare `f` only in leaving no `$?` | w03, w05 |
| `- f`, f `-> int` | the int | 74.48 |
| `$(- f)` | nothing: the capture is the same without the `-` | w16, w17 |
| `- f \|` | an authored status, or SIGPIPE | x05, x06, r02, r03 |
| `- f &` | nothing that is reported today (x07 = x08, x12 = x13); under F2, a fork failure | y13, y14 |

| Id | Program | Result |
|---|---|---|
| w03 | `fn g { return 7 }` then `- g` then `echo "after $?"` | SB5012 3:6 |
| w04 | `fn g { echo hi }` then `- g` then `echo after` | `hi`, `after` |
| w05 | `fn g { return 7 }` then `g` then `echo "after $?"` | `after 7` |
| w16 | `fn f -> int { return 7 }` then `echo "[$(- f)] [$(f)]"` | `[7] [7]` |
| w17 | `fn f { echo x; return 3 }` then `y=$(f)` then `echo "[$y] st=$?"` then `y=$(- f)` then `echo "[$y]"` | `[x] st=3`, `[x]` |
| x05 | `fn f { echo x; return 3 }` then `f \| /bin/cat` then `echo after` | `x`, rc 3, "f: command failed (exit 3)" |
| x06 | x05 with `- f \|` | `x`, `after` |
| x07 | x05's f, then `f &` then `- wait` then `echo after` | `x`, `after` |
| x08 | x07 with `- f &` | the same as x07 |
| x12 | x05's f, then `f &` then `- wait` then `int s=$(waitstatus)` then `echo "s=$s"` | `x`, `s=3` |
| x13 | x12 with `- f &` | the same as x12 |
| r02 | `fn gen { int i=0; while (( i < 200000 )) { echo "$i"; i+=1 } }` then `gen \| /usr/bin/head -1` then `echo done` | `0`, rc 141, "gen: command failed (exit 141)" |
| r03 | r02 with `- gen \|` | `0`, `done` |

- **(a) Recommended; DG's lean.** Reject `-` on a user-function call in
  every position: the statement, `$(- f)`, a stage and a job. The function
  is under the programmer's control "even in a child process", and the
  externals it runs take their own `-` inside it.
- (b) Allow it only where the call is a process (a stage or a job), whose
  status may be one nobody authored: SIGPIPE, or a fork failure. DG's
  "even in a child process" argues against it.
- (c) Keep in-process `- f` for a status or `-> int` function, and reject
  only the forms that discard nothing: `- void_f` (w04) and `$(- f)` (w16,
  w17).

To decide with it, the group spellings: `- { f }` (p03; p04,
`fn f { return 3 }` then `- { f }` then `echo done`, prints `done`),
`- { gen | /usr/bin/head -1 }` (p06) and `- { f & }` (p09, j1).

What the replacements cost:

- A status function: `f || :` (w23, `fn f { echo x; return 3 }` then
  `f || :` then `echo after`, prints `x`, `after`).
- An `-> int` function: `: $(( f ))` (u03 prints `side`, `done`). It
  works for `-> int` only: for a status function it skips the call (u10,
  batch-531 B77; see Related). If `- f` goes, DD#70's 2026-09-24 SB4030
  on `f || :` for an `-> int` function is up for review too.
- A producer stopped early by its reader dies of SIGPIPE (r02). Without
  `- gen |`:

  | Id | Program | Result |
  |---|---|---|
  | r4/r08 | `if gen \| /usr/bin/head -1 { echo t } else { echo "e $?" }` then `echo done` | `0`, `e 141`, `done` |
  | r4/r05 | gen with `/bin/echo "$i" \|\| return 0` in the loop | `0`, `done` |
  | r4/r04 | gen with `echo "$i" \|\| return 0` in the loop | `0`, rc 141, "gen: command failed (exit 141)" |
  | r4/r07 | `gen \| /usr/bin/head -1 \|\| echo "h $?"` | SB4023 2:1 "`&&` / `\|\|` on a pipeline isn't allowed — use `if pipeline; then …; fi`" |
  | p07 | `gen \| /usr/bin/head -1 \|\| :` | SB4023 2:1 |
  | p08 | `{ gen \| /usr/bin/head -1 } \|\| :` | SB4023 2:1 |
  | p01 | `- { gen } \| /usr/bin/head -1` | SB4022 2:3 |
  | p02 | `{ gen } \| /usr/bin/head -1` | SB4022 2:1 |
  | p06 | `- { gen \| /usr/bin/head -1 }` then `echo done` | `0`, `done` |

  The producer can stop itself only through an external (r4/r05): the
  builtin `echo` dies of the signal before its `||` runs (r4/r04).
- A job: `- f &` is coupled to Q5. Under F2 a fork failure of `f &`
  aborts (y13), and `- f &` discards it silently (y14, u06).

The converter is unaffected: it emits no `-` (convert.rs :638 matches `-`
only as `trap`'s argument), and it keeps bash's `f || true` and `f || :`.

Under (a), reverse: DD#55 :221; DD#70 :87–97 and :332–336; SB4016's
advice "discard with `-`" and `check_non_void_bare_call`'s early return
for a suppressed call (sash/src/transform/stmt_handlers.rs :168–185);
spec 74.48; and the 26 census ids (Census below).

### Q3. A bare call discards a status function's `return` (batch-531 B15)

DD#55 :217–222: "Any `return` statement makes the function non-void. Call
sites must explicitly handle the return value", and a "bare `myfunc`" is a
"**compile error** (unhandled non-void return)". Only `-> int` is
enforced: SB4016 rejects its bare call and its `&&`/`||` list tail alike
(l3, l4). A status function's bare call (w05) and list tail (l1, l2) are
silent.

Journal 253 :34–38 made a list tail count as a bare statement: "it is no
more a predicate than a bare `f` statement is, and a bare `f` was already
accepted." Its report and its specs (74.30–74.37) use void functions, so
it did not consider a status function.

| Id | Program | Result |
|---|---|---|
| w05 | `fn g { return 7 }` then `g` then `echo "after $?"` | `after 7` |
| w20 | `fn f { if [[ $1 == a ]] { return 3 }; echo "$1" }` then `f b` then `echo "after $?"` | `b`, `after 0` |
| l1 | `fn f { return 3 }` then `/bin/true && f` then `echo after` | `after` |
| l2 | `fn f { return 3 }` then `/bin/false \|\| f` then `echo after` | `after` |
| l3 | l1 with `fn f -> int` | SB4016 2:14 |
| l4 | l2 with `fn f -> int` | SB4016 2:15 |
| w23 | `fn f { echo x; return 3 }` then `f \|\| :` then `echo after` | `x`, `after` |
| w18 | `fn f { echo x; return 0 }` then `f` then `echo after` | `x`, `after` |
| w21 | `fn f { if [[ $1 == a ]] { return }; echo "$1" }` then `f a` then `f b` then `echo after` | `b`, `after` |
| w22 | w21's f, then `if f a { echo t } else { echo e }` | SB4037 1:1 |
| w15 | `fn descend { int n=$(($1)); if (( n == 0 )) { return 7 }; int next=$((n - 1)); return descend $next }` then `descend 2` then `echo "st=$?"` | `st=7` |
| w19 | `fn f { if [[ $1 == a ]] { return f b }; echo "$1" }` then `f a` then `echo after` | `b`, `after` |
| w01 | `fn g { return 7 }` then `echo "[$(g)]"` | `[]` |
| w02 | `fn g { echo hi; return 7 }` then `echo "[$(g)]"` | `[hi]` |
| w13 | `fn e { return 7 }` then `fn d { return $(e) }` then `echo "[$(d)]"` then `d` then `echo "st=$?"` | `[]`, `st=0` |
| w17 | (Q2) | `[x] st=3`, `[x]` |

The status is discarded differently by form: a stage aborts on it (x05,
n1), a bare statement is silent but leaves it in `$?` (w05), and a job is
silent but leaves it in `$(waitstatus)` (x12).

- **(a) Recommended.** Enforce DD#55: a bare call of a status function is
  a compile error, as SB4016 is for `-> int`. Journal 253's rule stands:
  a list tail is a bare call, so (a) rejects l1 and l2 as SB4016 rejects
  l3 and l4.
- (b) Abort when it returns nonzero, as a command does and as a stage
  already does (x05).
- (c) Amend DD#55 to say a bare call discards the status.

Sub-rulings for (a) and (b):

- A bare `return` is an early exit, not a status (w21). A predicate
  already needs every path to return (w22, SB4037).
- Tail calls are status-transparent (w15, w19): `return g x` passes g's
  status on, so f is a status function only if g is. This protects
  tailar.1–3, capcyc.1–2, stgcyc.1, pipeown.20, constw.11, .17 and .18,
  and 27.4 (27.4 is also a ceremony return; see the next bullet).
- `return 0` alone is ceremony, a compile error by DD#55 :224–225. Only
  `-> int` is checked (SB4017, sash/src/transform/statements.rs :254–272,
  which counts a bare `return` as `return 0`); a status function's
  ceremony is silent (w18). Enforcing it flips 27.4, fr.11, fr.12, cf.u4,
  tmpcap.19–21 and 88.11.
- Reading `$?` after a bare call is not handling it (w05, w20).
- A capture of a status function discards the status in every position
  (w01, w02, w13, w17); reject it. The c4 census finds no spec capturing
  a status function; the four that capture a tail-call function
  (tailar.3, capcyc.1, capcyc.2, constw.17) end in a void base case, so
  under the tail-call rule they capture a void function.

Under (a), ten specs that call a status function bare flip: 55.116,
rdr.39, rdr.40, skw.15, 74.67, 74.68, 74.69, 84.46, pf.35 and pf.36
(rdr.39 and 74.67 read `$?` after the call). No spec has a status
function as a list tail (c5; its control, widened to void functions,
finds journal 253's 74.30–74.32 and 74.34–74.37), so the l1/l2 form
flips none. `q-suppression-and-stage-residuals`
Q1's q1k/q1g note ("a function capture's own `return` status is unchecked
in every position") is this question.

### Q4. `-` reaches a call's argument captures

| Id | Program | Result |
|---|---|---|
| v05 | `/bin/echo "$(/bin/false)"` then `echo done` (`--no-taint`) | aborts |
| v03 | `- /bin/echo "$(/bin/false)"` then `echo done` (`--no-taint`) | an empty line, `done` |
| v04 | `str x=$(- /bin/echo "$(/bin/false)")` then `echo "[$x] done"` (`--no-taint`) | `[] done` |
| v07 | `if /bin/echo "$(/bin/false)" { echo t } else { echo f }` (`--no-taint`) | aborts |
| v06 | `if x=$(/bin/echo "$(/bin/false)") { echo "t[${x:!}]" } else { echo f }` (`--no-taint`) | aborts |
| c8 | `fn g { /bin/false; echo x }` then `- /bin/echo "a=$(g)"` then `echo done` | `a=x`, `done`, silent |

A statement `-` reaches an argument capture (v03, v04), and through it a
function's body (c8); a condition does not (v06, v07). A capture is a
body (DD#86, 2026-09-29), so this is Q1's boundary again.

- **(a) Recommended.** No reach (DD#86). Batch-531 B76 (v06 aborts,
  where DD#55's 2026-10-20 text says a capture condition suppresses the
  whole capture) is then not a bug, and DD#55 :67–81 is corrected. Flips
  supval.6, .7, .8, .14 and .18–.21.
- (b) Keep the reach. B76 is then a bug (v06 should not abort), and its
  fix lands with F6.

### Q5. Setup failures, and what "maybe a system call result" covers

DD#70 :149–161: a redirect's `open` "is a checked syscall that runs
**first**"; it "aborts unless it sits in a **condition** (`if`/`||`)",
and "**`-` does not reach the open** … So `- echo > /bad` still
**aborts**."

`fn f { echo x }` unless a row says otherwise; each program ends with
`echo after`.

| Id | Program | Result |
|---|---|---|
| y08 | `- /bin/echo x >ND` | rc 1, ND's message |
| y11 | `- echo x >ND` | rc 1, ND's message |
| y01 | `f >ND` | rc 1, ND's message |
| y02 | `- f >ND` | rc 1, ND's message |
| y07 | `fn f -> int { echo x; return 3 }` then `- f >ND` | rc 1, ND's message |
| e1 | `if /bin/echo x >ND { echo t } else { echo "e $?" }` | `e 1`, `after`, silent |
| e2 | `/bin/echo x >ND \|\| echo "h $?"` | `h 1`, `after`, silent |
| e3 | `fn f { echo x; return 0 }` then `if f >ND { echo t } else { echo "e $?" }` | `e 1`, `after`, silent |
| e4 | `if echo x >ND { echo t } else { echo "e $?" }` | `e 1`, `after`, silent |
| e5 | e3's f, then `f >ND \|\| echo "h $?"` | `h 1`, `after`, silent |
| b2 | `cd /nonexistent/d` | rc 1, "cd: /nonexistent/d: No such file or directory" |
| b1 | `- cd /nonexistent/d` | `after`, silent |
| b3 | `cd /nonexistent/d \|\| echo "h $?"` | `h 1`, `after` |

As a stage (each program ends with `echo done`, y10 with `echo after`):

| Id | Program | Result |
|---|---|---|
| y10 | `- f >ND \| /bin/cat` | `after`, silent |
| z02 | `/bin/cat < missing \| /bin/cat` | rc 1, "missing: No such file or directory", "/bin/cat: command failed (exit 1)" |
| z01 | `- /bin/cat < missing \| /bin/cat` | `done`, silent |
| z04 | `fn f { /bin/cat }` then `f < missing \| /bin/cat` | rc 1, "missing: No such file or directory", "f: command failed (exit 1)" |
| z03 | z04 with `- f < missing \|` | `done`, silent |
| z06 | `echo data < missing \| /bin/cat` | rc 1, "missing: No such file or directory", "echo: command failed (exit 1)" |
| z05 | `- echo data < missing \| /bin/cat` | `done`, silent |
| z07 | `- /bin/cat < missing` (no pipe) | rc 1, "missing: No such file or directory" |
| z09 | `/bin/echo x \| /bin/cat < missing` | rc 141, "missing: No such file or directory", "/bin/echo: command failed (exit 141)" |
| z08 | `/bin/echo x \| - /bin/cat < missing` | rc 141, only "/bin/echo: command failed (exit 141)" |
| z10 | `fn f { /bin/cat < missing; echo "in $?" }` then `- f \| /bin/cat` | `done`, and "missing: No such file or directory"; no `in` line |

In a capture and in a job:

| Id | Program | Result |
|---|---|---|
| k06 | `str x=$(/bin/cat <ND)` then `echo "[$x] after"` (`--no-taint`) | rc 1, ND's message |
| k05 | `str x=$(- /bin/cat <ND)` then `echo "[$x] after"` (`--no-taint`) | rc 1, ND's message |
| k07 | `/bin/cat <ND &` then `- wait` then `echo after` | `after`, and the child's ND message |
| k08 | k07 with `- /bin/cat <ND &` | the same as k07 |
| x17 | `/bin/false &` then `- wait` then `int s=$(waitstatus)` then `echo "s=$s"` | `s=1`, and the child's message |
| x16 | x17 with `- /bin/false &` | `s=1`, silent |

Fork failures. The first `clone` fails with EAGAIN. `f` is
`fn f { echo x }` unless the row defines it. Each job is followed by
`- wait`, and each program ends with `echo after` (the u rows with
`echo done`) unless the row shows its own ending:

| Id | Program | `899179d` | F2 trial |
|---|---|---|---|
| y17 | `/bin/echo x \| /bin/cat` | rc 1, "fork: error 11" | the same |
| k02 | `- /bin/echo x \| /bin/cat` | rc 1, "fork: error 11" | the same |
| k01 | `- /bin/echo x \| - /bin/cat` | rc 1, "fork: error 11" | the same |
| k04 | `- /bin/echo x` | `after`, and "/bin/echo: error 11" | the same |
| k03 | `str x=$(- /bin/echo x)` then `echo "[$x] after"` (`--no-taint`) | `[] after`, silent | the same |
| y13 | `f &` | `after`, silent | rc 1, "fork: error 11" |
| y15 | `/bin/echo x &` | `after`, silent | rc 1, "fork: error 11" |
| u05 | `fn f { return 3 }` then `f &` | `done`, silent | rc 1, "fork: error 11" |
| u04 | `fn f { return 3 }` then `f \|\| : &` | `done`, silent | rc 1, "fork: error 11" |
| j2 | `{ f & }` | `after`, silent | rc 1, "fork: error 11" |
| y14 | `- f &` | `after`, silent | the same |
| y16 | `- /bin/echo x &` | `after`, silent | the same |
| u06 | `fn f { return 3 }` then `- f &` | `done`, silent | the same |
| j1 | `- { f & }` | `after`, silent | the same |
| j4 | `- { /bin/echo x & }` | `after`, silent | the same |

DG's "maybe a system call result" reads two ways:

- A builtin that wraps one syscall. `-` already discards its failure, as
  `||` does (b1, b3).
- A setup syscall a command needs before it runs: a redirect's `open`, a
  stage's or a job's `fork`. In the foreground DD#70 keeps these out of
  `-`'s reach (y08, y11, y02, k05) while a condition handles them
  (e1–e5). A stage `-` discards them silently, though (y10, z01, z03,
  z05), and so does a job `-` under F2 (y14, y16, j1, j4).

Options:

- (a) `-` never discards a setup failure, in any position; on a stage or
  a job it discards only the process's own status.
- (b) `-` discards a setup failure wherever `||` or a condition handles
  one (e1–e5). Reverses DD#70 :159–161.
- (c) Keep the split, and document which positions discard one.

No recommendation: the choice turns on which reading DG meant, and on
Q2, which decides whether y14 and u06 compile at all (j1 is a group
spelling). The messages are inconsistent whatever the ruling: z08 hides
the open failure but reports the SIGPIPE it causes; k04 continues with a
message and k03 silently; a job `-` does not silence the child's report of
its open failure (k07 = k08), but it does silence the report of its exit
status (x16, against x17). `bug-fork-failure-reporting-by-form` holds the
fork rows; the ruling here decides which of them `-` may silence.

### Q6. A pipe whose producer never writes, or whose consumer never reads

| Id | Program | Result |
|---|---|---|
| n1 | `fn f { return 3 }` then `f \| /bin/cat` then `echo after` | rc 3, "f: command failed (exit 3)" |
| n2 | `: \| /bin/cat` then `echo after` | `after` |
| n6 | `fn f { /bin/true }` then `f \| /bin/cat` then `echo after` | `after` |
| n4 | `fn g { echo y }` then `/usr/bin/yes \| g` then `echo after` | `y`, rc 141, "/usr/bin/yes: command failed (exit 141)" |
| n5 | n4 with `- /usr/bin/yes \| g` | `y`, `after` |

- (a) DG's lean, for what the compiler knows in-process: a stage known
  never to write to stdout is an error on a pipe's left (n1, n2), and one
  known never to read stdin is an error on its right (n4's g). n6 stays
  legal: `/bin/true` is an external. The left half needs W2's T1
  reaches-stdout analysis; the right half needs a new reads-stdin
  analysis. Flips pf.51, p518.10, p518.28, p518.29, pipeown.32 and 24.29,
  each of which pipes `:`.
- (b) Keep these pipes legal.

It cannot land before W2. DG's taint remark ("`cmd "$(g)" | cat` should
be a taint error unless g is known to be clean") is W2's T1
(`batch-531-findings` §18), already planned. F2's new specs give every
function stage a write (x05's shape, not n1's), so this ruling does not
churn them.

### Census

c3 finds 26 spec ids that put `-` on a user-function call: rarg.9 (asserts
SB5037); rdr.3, .5 and .19 (assert SB4046); supval.1–.6, .9–.12, .14–.17,
.22, .26 (asserts SB4025) and .28; 74.48; tmpcap.19–21 (.21 asserts
SB5037); and constw.19. Six more reach a function only through an argument
capture: supval.7, .8 and .18–.21. No spec puts `-` on a function as a
stage or a job.

- Q1(a) flips supval.1–.5, .9, .12 and .22, and supval.11 needs a new
  program. supval.17 still passes, but only because its `- speak` aborts
  on its own; supval.10, .15, .16 and .28 already abort.
- Q2(a) needs all 26 rewritten.
- Q4(a) flips supval.6, .7, .8, .14 and .18–.21.
- Q3's flips are listed under Q3; Q6's under Q6.

### Related

- Batch-531 B82 is a bug only if Q1 and Q4 keep the reach.
- Batch-531 B81 (a stage's argument captures ran in the parent): every
  row below makes the compiler panic at `899179d` (exit 1 native, 101 C:
  "ICE: native|c lowered a value as a scalar, but FuncCall (g) should have
  been lifted during transform"). F2 moves the captures into the stage's
  child. A stage `-` then does not reach an argument capture's function
  (c1, c7, c9), while a statement `-` does (c8); Q4 decides which is
  right.

  | Id | Program | `899179d` | F2 trial |
  |---|---|---|---|
  | c1 | `fn g { /bin/false; echo x }` then `- /bin/echo "a=$(g)" \| /bin/cat` then `echo done` | ICE | `done`, and the message |
  | c2 | c1 without the `-` | ICE | rc 1, the message, "/bin/echo: command failed (exit 1)" |
  | c7 | c1's g, then `echo hi \| - /bin/grep "$(g)"` then `echo done` | ICE | `done`, and the message |
  | c9 | c1's g, then `/bin/echo hi \| - /bin/grep -c "$(g)" \| /bin/cat` then `echo done` | ICE | `done`, and the message |
  | c10 | `fn g { echo x }` then `/bin/echo "a=$(g)" \| /bin/cat` then `echo done` | ICE | `a=x`, `done` |
  | c11 | c10 with `echo` for `/bin/echo` | ICE | `a=x`, `done` |

- Batch-531 B77, extended (a bare name in `$(( ))` that names a non-int
  function; one DD#58 collision authority, Order item 19). It matters here
  because `: $(( f ))` is Q2's replacement for an `-> int` function only.
  `fn f { echo side; return 3 }` unless a row says otherwise:

  | Id | Program | Result |
  |---|---|---|
  | u03 | `fn f -> int { echo side; return 7 }` then `: $(( f ))` then `echo done` | `side`, `done` |
  | u10 | `: $(( f ))` then `echo done` | `done`, no `side` |
  | u08 | `int n=$(( f ))` then `echo "n=$n"` | `n=0` |
  | u07 | `fn f { return 3 }` then `int n=$(( f ))` then `echo "$n"` | `0` |
  | u11 | `fn f { echo side }` then `int n=$(( f + 1 ))` then `echo "n=$n"` | `n=1` |
  | u14 | `fn f { return 3 }` then `int n=$(( f + 1 ))` then `echo "n=$n"` | `n=1` |
  | u09 | `int n=$(( (f) ))` | SB4030 2:7 |
  | u13 | `fn f { return 3 }` then `int n=$(( (f) ))` | SB4030 2:7 |
  | u12 | `int n=$(( nosuch + 1 ))`, with no f | SB5031 1:7 |
  | a1 | `int n=$(( echo + 1 ))`, with no f | SB4030 1:7 |
  | a3 | `if (( f )) { echo t } else { echo e }` | `e` |
  | a4 | `(( f ))` then `echo "after $?"` | `after 1` |
  | a8 | `int n=0` then `(( n = f + 2 ))` then `echo "n=$n"` | `n=2` |

- B121 (new): a void function escapes SB4037 when its call is redirected
  (y05, y06, y12, y18, y19). It is not a suppression question;
  `bug-peel-family-classifiers` holds it.
- Spec 55.15 (spec/errors.sh :238) asserts only
  `rc != 0 && -z $out && -n $err`. Its program,
  `fn f -> int { - /bin/false; return $?; }`, is SB5012 1:36 (d55/s15),
  and the void twin under `if f` is SB5012 1:29 (d55/s15b).
  `dd55-nonvoid-functions` records it.
- `q-suppression-and-stage-residuals` asks the same question of `-` on a
  command where its status is read (its Q3, `if - cmd`), and of a `-`
  that leaves the statement with no effect (its Q5, `- [[ … ]]`,
  `- match`). Read a ruling here against those.
