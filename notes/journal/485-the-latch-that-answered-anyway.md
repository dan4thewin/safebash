---
title: The latch that answered anyway
date: 2026-10-09
---

# 485 — The latch that answered anyway

DD#126 shipped yesterday with a "Known gap" section. The gap was that
`while (( wait > 0 )) { int s=$(status) }` could not read the reaped child's
status, because an arithmetic condition sets `$?` from its own verdict. I
wrote it up as cosmetic and additive: "closing it would mean a `waitstatus`
intrinsic, which is purely additive and not done here."

It was not cosmetic. The drain loop is the whole reason `wait` returns a pid
instead of aborting, and the body of that loop is where you look at how each
child did. The gap *was* the feature.

## The fork, and why the out-param lost

Two shapes could close it: a `waitstatus` reader, or `wait` taking a name to
assign the status into. I measured before recommending, and three of the
measurements decided it.

`if /bin/sh -c "exit 7" {} else { int s=$(status) }` gives **7** — the real
status, not the condition's verdict. So "conditions should not clobber `$?`"
is not an available fix; only *arithmetic* conditions are tautological, and a
blanket rule would destroy real information in every else-branch.

`- wait; echo x; int s=$(status)` gave **0** — silently wrong. The wait status
was already a trap, independently of the loop.

And the out-param dies on the motivating case. The drain loop is the *sexpr*
form, where `(wait p)` already means the value of `p`. A second operand
`(wait p s)` would be a *name*, reintroducing exactly the two-operand-semantics
split DD#126 had just removed from `reconstruct_intrinsic_command`. An
out-param can only work in the `- wait` statement form — the one shape that
does not need it.

The clincher was subtraction rather than addition. `waitstatus` lets `wait`
stop writing `$?` entirely, which **deletes** the SB5012 exception
`stmt_reaps_child` that §1 had added. `-` means "I am not handling this"
again, with no carve-out. DG took the recommendation and added one
requirement: keep `status` / `getcpid` / `waitstatus` canonical, with `$?` and
`$!` as sugar.

## The second latch was already broken

Writing the rule meant noticing that `$(waitstatus)` has a twin. `$!` is also
a single-slot compiler-managed value, also silently overwritten, and
`notes/tasks/bug-bg-pid-slot-unavailable.md` had it filed as a P1: `echo $!`
before any `&` compiled and yielded an empty string.

The task offered DG two options — mirror SB5012, or a `-1` sentinel. The
sentinel is the cheaper one and it is what DD#126 chose for ECHILD, so
consistency argued for it. But it only fixes the *first* read. `&`, then ten
statements, then `$!` still answers with a plausible wrong number, and that is
the worse half: a silent empty string at least looks wrong.

So one rule covers both. **SB5094: a latch is readable only in the statement
immediately following the one that wrote it.** Not a liveness analysis, not a
dataflow — a one-statement window, which is all either value was ever
meaningfully alive for.

The drain loop then works with no special case at all, because
`check_dq_while` already seeds the body with the state the condition left.
The condition *is* the producing statement. That was luck, but it was the kind
of luck that tells you the shape is right.

## Two bugs

**SB5012 could not see a `(( … ))` statement.** Not the expression form — that
was caught, and `find_status_reads` has a long doc comment about catching it.
The *statement* form. `Statement::direct_exprs` yields nothing at all for
`StmtKind::ArithTest`, deliberately: its children hang off an `ArithExpr`, and
the passes that want them also want nodes that are not `Expr`s, so they walk
the arith tree themselves. Every check that asks "does this statement name
intrinsic X" therefore has to ask the arith tree separately, and two did not.

```sh
if (( (status) == 0 )) { echo y }     # compiled. No preceding command.
```

I found it only because my reap predicate had the identical blindness and the
drain loop failed. The fix is one shared helper; the status read and the reap
and the latch read now all route through it.

**`- cmd &` never reaches the list arm.** It parses as
`Suppressed(List(Background, …))` — the suppression wraps the whole list — so
a latch transition written into `check_dq_list` fires for `cmd &` and not for
`- cmd &`. My first cut rejected `- cmd & ; int p=$!`, which is the single
most ordinary thing anyone writes with a background job. The repair was to
stop having two answers: `latch_after` is structural and is applied to every
statement shape in one place.

Both faults are the same fault, really. A property of a *statement* was being
computed from the shape the walker happened to be looking at, rather than from
the statement.

## The exit status that was an accident

`spec/errors.sh` 55.16 failed, and it took a while to believe the failure was
correct.

```sh
/nonexistent_binary_xyz &
- wait
```

used to exit 127. It now exits 0. The 127 was never designed: `- wait` left
the reaped status *available* under the §1 exception, and an
available-but-unconsumed status at the end of a program is the program's exit
status. Remove the exception and the accident goes with it.

I do think the new behaviour is right — the author wrote `-` and read no
latch, which is saying "I do not care" twice — but it is a visible change in
what a failing background job does to a program's exit code, so it is in the
DD and in two specs rather than quietly rebaselined. 126.36 pins the spelling
that propagates it.

## Numbers

Sweep 7997 (predicted 7997: 16 new specs × 2 modes on 7965). 0 failing, 0
regressions, 32 new ids and all 32 expected. Self-test 248, so the regression
query can still fail. `mk test` green.

The complexity gate caught `find_latch_reads` at cog 7 against a 3× bar of
5.22 — a `for` over two intrinsics wrapped around a closure holding two `if`s.
Split into the loop and the per-intrinsic body: 2/1 and 4/3.
