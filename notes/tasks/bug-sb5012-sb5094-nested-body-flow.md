---
title: "Bug: SB5012/SB5094 check nested executable bodies against the enclosing statement's state, not the body's own flow"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "anonymous capture bodies start with unavailable status and no inherited latch; caller-derived reads get SB5012/SB5094 regardless of spelling, outer handling or nesting"
  - "legal handling, spawn/reap and latch reads in the body's own flow remain valid, as does access to a status saved into an enclosing binding"
  - "direct status/getcpid/waitstatus value expressions retain their surrounding context and are not mistaken for executable capture bodies"
  - "every still-legal false row compiles and runs as its twin does, in both backends"
  - "every still-legal missed row draws its twin's code"
  - "still-legal controls keep their results; changed source contracts are migrated by their policy owners"
  - "focused specs pin each distinct defect in both backends with pre-fix or targeted reversion failures; preservation controls and the predicted ordinary gate pass"
  - "group-only rows become retire-command-groups refusals; consumed if/loop rows become q-compound-as-condition refusals, not claimed nested-flow fixes"
---

## Entry policy settled, 2026-11-07

DG chose **"no inheritance"**: an anonymous capture body starts without
the caller's `$?`, like a named function body. Its own handling flow can
make status available. DD55/DD86 and journal 586 record the decision;
DD126's existing fresh-function latch rule applies to captures too.
The entry-state question is closed, but this P2 implementation bug remains
open. No checker repair is included in the decision recording.

Freshness belongs to executable body entry, not every `$()` token.
Direct `$(status)`, `$(getcpid)` and `$(waitstatus)` are value reads in the
surrounding context. Inside a real capture body, those same reads use its
own context and cannot borrow an outer permission. Preserve the canonical
punctuation and arithmetic spellings. Saving a status at a legal outer
handling position and reading that ordinary binding inside remains valid.

## Evidence scope

DD68's 2026-11-09 no-result rule also settles the consumed `if`/loop
cases: they must reject (`q-compound-as-condition`, journal 597).
Group retirement and compound-suppression refusals retain their own
owners. This task still covers legal capture/reader bodies and ordinary
statement execution, including the conditional-block shorthands through
their `if` flow. An earlier refusal is not evidence that those bodies'
status/latch checks were repaired.

The tables below retain the original measurements, not new results.
Revalidate them against the current DDs when implementing, including
separately settled return-kind and compound-suppression changes. The
entry-state table now specifies the required refusals; the historical
claim that no existing spec would change is not a sweep prediction.

## Found

2026-10-27, while porting F1+F4 (journal 537). Pre-existing at `8e28c61`,
unchanged by F1+F4, and the same on `taint-walker-redesign` (`e98fca8`); both
backends. A *twin* is the same flow written where the checker follows it: in a
function body, at top level, or without the wrapper.

`⏎` marks a newline.

### Capture bodies

False rejections:

| Program | Result | Twin |
|---|---|---|
| cb1 `echo "$(echo a; echo $?)"` | SB5012 1:22 | cb7 `fn g { /bin/true; echo "in=$?" } ⏎ g` prints `in=0` |
| cb2 `x=$(/bin/true; echo "in=$?") ⏎ echo "${x:!}"` | SB5012 1:21 | cb7 |
| cb3 `x=$(if /bin/sh -c "exit 3" { echo t } else { echo "in=$?" }) ⏎ echo "${x:!}"` | SB5012 1:51 | cb6 (the same `if` in `fn g`) prints `in=3` |
| cb4 `fn g { x=$(/bin/true; echo "in=$?"); echo "${x:!}" } ⏎ g` | SB5012 1:28 | cb7 |
| cl2 `x=$(- /bin/true & ⏎ - wait $! ⏎ echo "[$(waitstatus)]") ⏎ echo "${x:!}"` | SB5094 2:8 and 3:6 | cl4 (the same body in `fn g`) prints `[0]` |
| cl9 `- /bin/sh -c "exit 4" & ⏎ x=$(int tot=0 ⏎ while (( wait > 0 )) { int s=$(waitstatus); tot=$(( tot + s )) } ⏎ echo "tot=$tot") ⏎ echo "${x:!}"` | SB5094 3:30 | cl10 (the same body in `fn g`) prints `tot=4` |

Missed:

| Program | Prints | Twin |
|---|---|---|
| cb8 `echo hi ⏎ x=$(- /bin/false; echo "in=$?") ⏎ echo "${x:!}"` | `hi`, `in=1` | cb10 `- /bin/false ⏎ echo "[$?]"` SB5012 2:6; cb12 (in `fn g`) 1:27 |
| cb9 `echo hi ⏎ x=$(/bin/sh -c "exit 3" \|\| :; echo "in=$?") ⏎ echo "${x:!}"` | `hi`, `in=0` | cb11 `/bin/sh -c "exit 3" \|\| : ⏎ echo "[$?]"` SB5012 2:6; cb17 (in `fn g`) 2:6 |
| cb18 `x=$(if (( (status) == 0 )) { echo y } else { echo n }) ⏎ echo "${x:!}"` | `y` | cb19 (top level) SB5012 1:4; cb20 (in `fn g`) 1:11 |
| cl3 `- /bin/true & ⏎ x=$(echo hi; echo "$!") ⏎ if [[ -n "${x:!}" ]] { echo nonempty }` | `nonempty` | cl6 `fn g { - /bin/true & ⏎ echo hi ⏎ echo "$!" } ⏎ g` SB5094 3:6 |
| cl7 `x=$(if (( (waitstatus) == 0 )) { echo y } else { echo n }) ⏎ echo "${x:!}"` | `n` | cl8 (top level) SB5094 1:4 |

Controls:

| Program | Result |
|---|---|
| cb5 `echo hi ⏎ echo "$(echo a; echo $?)"` (the shape of 23.31) | prints `hi`, `a`, `0` |
| cb21 `x=$(echo "[$(( (status) ))]") ⏎ echo "${x:!}"` | SB5012 1:10 |

Each verdict above is taken from whatever precedes the enclosing statement:
cb1 and cb5 differ only in a leading `echo hi`.

### A pipeline's reader body

`${x:!}` keeps `bug-validator-taints-clean-pipe-while-reader`'s false SB5020
out of the way (w5, with a plain `$x`, is SB5020 2:48).

| Program | Result | Twin (no pipe) |
|---|---|---|
| p1 `echo a \| while readline x { /bin/true; echo "in=$?"; echo "${x:!}" }` | SB5012 1:45 | p1t: legal |
| p2 `echo hi ⏎ echo a \| while readline x { - /bin/false; echo "in=$?"; echo "${x:!}" }` | prints `hi`, `in=1`, `a` | p2t: SB5012 2:39 |
| l3 `echo a \| while readline x { - /bin/true & ⏎ - wait $!; echo "${x:!}" }` | SB5094 2:8 | l3t: legal |
| b17d `/bin/printf "1\n2\n" \| while readline x { attest x [[ -n $x ]] ⏎ echo "got $x" & ⏎ int p=$! ⏎ int r=$(wait $p) ⏎ int s=$(waitstatus) ⏎ echo "s=$s r=$(( r == p ))" } ⏎ echo done` | SB5094 4:9 and 6:9, every version (found in F2, journal 539; `files/port-f2-probes.tar.gz` `sp/b17d.sh`) | legal |

### `-`-wrapped compounds

At a statement's head `-` takes the whole statement (DD55 addendum, line 373),
and the body runs that way: `- if /bin/true { /bin/false; echo after }`, its
`for` and `while` forms (s1-s3) print `after` and carry on.

| Program | Result | Twin |
|---|---|---|
| p3 `echo hi ⏎ - if /bin/true { - /bin/false; echo "in=$?" }` | prints `hi`, `in=1` | w2t (no `-`): SB5012 2:35 |
| p5 `echo hi ⏎ - if /bin/true { /bin/false; echo "in=$?" }` | prints `hi`, `in=1` | cb10: SB5012 2:6 |
| l2 `- /bin/true & ⏎ - if /bin/true { echo "$!" }` | prints a pid | l2t (no `-`): SB5094 2:21 |
| w3 `- if /bin/sh -c "exit 3" { echo t } else { echo "in=$?" }` | SB5012 1:49 | w0 (no `-`) prints `in=3` |
| w3b: w3 after `echo hi` | prints `hi`, `in=3` | — |
| p4 `- for i in a b { /bin/true; echo "$? $i" }` | SB5012 1:34, "no preceding command" | — |
| p4b: p4 after `echo hi` | prints `hi`, `0 a`, `0 b` | cb10: SB5012 2:6 |

Two readings here are decide-and-report:

- **w3 is a false SB5012.** `-` discards the statement's status for the
  statements after it. The condition's status still feeds its own branch, as
  DD55:41-47 has it, and w3b reads 3 at run time.
- **p4's SB5012 is right for the wrong reason.** Every command under the `-` is
  suppressed, so DD55:26-29 makes a `$?` read after one a compile error, as
  for `- cmd` (cb10). The message blames a missing command. p4b and p5 are
  missed.

l1 `- if /bin/true { - /bin/true & ⏎ - wait $! }` is SB5094 2:8, but so is its
plain twin l1t: that is `bug-handling-branch-drops-latch`, not this task.

### Negated compounds (historical; now require rejection)

General-group rows now require rejection under `retire-command-groups`
(DD70, journal 596). The consumed `if`/loop rows also require rejection
under `q-compound-as-condition` (DD68, journal 597): `!` cannot consume
a statement's nonexistent result. The un-negated twins n3t and n4t
were SB4023.

The accepted `&&`/`||` block shorthands must instead follow their
ordinary `if` flow. An earlier group-syntax refusal does not prove a
status/latch-flow fix, nor does the new result-use refusal; preserve
that evidence with still-legal bodies.

| Program | Result | Twin |
|---|---|---|
| n3 `echo hi ⏎ ! { - /bin/false; echo "in=$?" } && echo y` | prints `hi`, `in=1` (exit 1: the list is false) | cb10, cb12: SB5012 |
| n4 `! { /bin/true; echo "in=$?" } \|\| echo n` | SB5012 1:21 | cb7: `in=0` |
| rp1 `fn f { return ! { /bin/true; echo "in=$?" } } ⏎ if f { echo y } else { echo n }` | SB5012 1:35 | cb7 |
| rp2 `fn f { /bin/true ⏎ return ! { - /bin/false; echo "in=$?" } } ⏎ if f { echo y } else { echo n }` | prints `in=1`, `n` | cb12: SB5012 |
| nf1 `if ! for x in a { /bin/true; echo "in=$? $x" } \|\| /bin/true { echo y }` | SB5012 1:35 | p4t: `0 a`, `0 b` |
| nf2 `echo hi ⏎ if ! for x in a { - /bin/false; echo "in=$? $x" } \|\| /bin/true { echo y }` | prints `hi`, `in=1 a`, `y` | cb10: SB5012 |

Out of scope: a `!`-wrapped group as a header condition is SB4027 since F1+F4
(n1, n2; before it, n1 was SB5012 1:24 and n2 printed `in=1`). A redirected
compound reader loop is SB4021 (r1, r2).

Probes: `files/port-f1f4-probes.tar.gz`, directories `cb/` (cb*, cl*, ev*),
`wr/` and `wr2/`.

## Why it is a bug

- DD86 makes an executable `$( )` body an anonymous function body with its
  own validation and lexical context, subject to the capture's result
  contract. The 2026-11-07 entry rule is now explicit too. The historical
  capture rows and their `fn` twins get different flow verdicts.
- DD126 requires a fresh function context and a latch read in the statement
  immediately after its write. cl2, cl9 and l3 write and read inside one body
  and are rejected. cl3, cl7 and l2 read a latch that this rule makes stale
  there (l2's read is inside an `if` body, which DD126 says the latch does
  not cross), and are accepted.
- DD55 forbids `$?` after `- cmd` and requires it to be the first evaluated
  expression in a legal handling position. A wrapper cannot grant the
  body's reads permission from an unrelated enclosing statement.

## Cause at filing

`check_dq_stmt` (validate.rs 3311-3330) has its own arms for List, Group,
Function, If, While, For and Case. Every other kind goes to `check_dq_leaf`
(3452-3466), and the statement's latch then comes from `latch_after(stmt)`
(3329). Four shapes reach the leaf arm with a body inside them:

- a `Suppressed` compound (`- if …`);
- a negated compound as a list operand or a `return` producer (a `!`-wrapped
  Pipeline);
- a Pipeline whose final stage is the reader loop;
- any statement holding a capture.

The leaf judges every read below it against the statement's incoming state:

- `$?`: `stmt_uses_status` (3701-3705) → `find_status_reads` (3664-3673) →
  `walk_exprs_in_stmt` (3230-3232) → `ir::walk_all_exprs` (ir.rs 389-396).
  That recurses through `child_bodies` (91-95), which is `structural_bodies`
  (119-143) plus `cmdsub_bodies` (107-115).
- The latch: `check_latch_reads` (3470-3482) → `find_latch_reads`
  (3503-3507) → `find_one_latch_read` (3517-3526), the same walk.
- `(( ))` statements in a nested body are not seen at all.
  `arith_test_names_intrinsic` (3692-3699) looks only at the top statement's
  own kind (cb18, cl7; and cl9's `while (( wait > 0 ))` is not a reap).

The model to follow is `check_dq_function` (3380-3385): a fresh
`FlowState::new(StatusState::Undefined)` and `check_dq_stmt` for each
statement. `latch_after` (3339-3347) and `stmt_backgrounds_job` (3351-3358)
already peel Suppressed and Redirected. The other dispatchers are
`check_dq_list` 3362-3370, `check_dq_if` 3391-3407, `check_dq_while`
3412-3421, `check_dq_for` 3427-3433, `check_dq_case` 3438-3446,
`check_dq_stmt_branch` 3729-3740 and `check_dq_body_branch` 3743-3754.

## Fix direction

- Give each executable capture body a fresh status/latch `FlowState`, as
  named bodies already have. Check its statements in order rather than
  judging its reads against the enclosing statement. Nested bodies start
  fresh in turn.
- Stop the enclosing status/latch read collectors at executable body
  boundaries, using the existing shared body/expression classification.
  Do not treat direct intrinsic value reads as new bodies merely because
  their spelling or AST uses command substitution.
- Structural branches and reader loops still derive their entry flow from
  their own conditions and existing DD55/DD126 rules. Do not initialize
  every block as a fresh function or erase a body's internally established
  handling status.
- Peel `-` and model what it discards: a `$?` read after a suppressed command
  is SB5012, but a condition still feeds its own branch (w3, w3b).
- Check a pipeline's reader body as a plain `while` body (p1t, p2t and l3t
  are the oracle), and a negated compound's body as the plain compound's.
- Cover both codes, and the `(( ))` statements in each body.

## Capture-entry regressions required by the ruling

No outer status or latch permission enters the body. The caller's
left-to-right order remains independently required, but no ordering can
make these inherited body reads legal. Nor should a runtime zero replace
the missing permission.

| Program | Recorded result | Required under the entry rule |
|---|---|---|
| cb13 `echo hi ⏎ x=$(echo "in=$?") ⏎ echo "${x:!}"` | prints `hi`, `in=0` | SB5012 inside the capture |
| cb16 `if /bin/sh -c "exit 3" { echo t } else { x=$(echo "in=$?"); echo "$x" }` | prints `in=3` | SB5012 inside the capture |
| cl1 `- /bin/true & ⏎ x=$(echo "$!") ⏎ if [[ -n "${x:!}" ]] { echo nonempty }` | prints `nonempty` | SB5094 inside the capture |
| ev12 `/bin/sh -c "exit 3" \|\| echo "$(echo "in=$?")"` | prints `in=3` | SB5012 inside the capture |
| ev14 `fn t { /bin/true } ⏎ /bin/sh -c "exit 3" \|\| echo "$(echo "in=$?")" "$(t)"` | prints `in=0`: `$(t)` runs first (bash prints `in=3`) | SB5012 inside the capture |
| ev13 `fn t { /bin/sh -c "exit 5" \|\| : } ⏎ /bin/sh -c "exit 3" \|\| echo "$(t)" "$(echo "in=$?")"` | prints ` in=0`, the status `$(t)` left | SB5012 inside the capture |
| cl5 `fn g { echo "$!" } ⏎ - /bin/true & ⏎ x=$(g) ⏎ …` | SB5094 1:13: a named body does not inherit | Preserve the named-body refusal |
| cb14 `echo hi ⏎ fn f { echo "in=$?" } ⏎ f` | SB5012 2:13 | Preserve the named-body refusal |
| cb15 `echo hi ⏎ x="in=$?" ⏎ echo "$x"` | prints `hi`, `in=0` | No capture body; existing outer DD55 rules are unchanged |

ev13 and ev14 were run on `8e28c61` and the F1+F4 tree. The branch rejects
both with SB5020 (B114).

Required positive controls include a capture's own handled failure
(cb3/cb6), fresh body-local spawn/reap latches (cl2/cl4), and a wait inside
the body establishing a new latch after reaping an existing process-owned
child (cl9/cl10). Preserve direct status/latch intrinsic reads in legal
outer contexts and a status explicitly saved into an enclosing binding.
Exercise nested bodies and equivalent punctuation/intrinsic/arithmetic
reads without allowing an unrelated diagnostic to satisfy the assertion.
Choose fresh spec ids and predict the sweep from the current database,
not from this filing's old counts.

## Not this task

- Left-to-right evaluation (ev5-ev11): the accepted rule's implementation
  is in `q-capture-hoisting-order`, `batch-531-findings` §12. ev12-ev14's
  inherited reads now belong to this task's refusal coverage instead.
- The latch dropped after a branch's first statement (l1):
  `bug-handling-branch-drops-latch`.
- What SB5012 says ("no preceding command"):
  `bug-sb5012-message-names-no-command`.
- The false SB5020 on a clean pipe's reader variable (w5):
  `bug-validator-taints-clean-pipe-while-reader`.
- Refusing consumed `if`/loop results, including through `!`:
  `q-compound-as-condition` (policy settled, implementation pending).
