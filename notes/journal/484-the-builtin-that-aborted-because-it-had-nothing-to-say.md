# 484 — the builtin that aborted because it had nothing to say

DD#125 reserved the names; journal 483 made the bare word name the intrinsic.
This is the third part DG settled in the same conversation, and the one with
an actual bug under it.

## The idiom that could not be written

The shape DG wanted is

```sh
while (( (wait) > 0 )) { : }
```

and it could not be written, because `wait` was a statement builtin that
**aborted** when it found no children. The loop's exit condition was the
program's exit. Bash papers over this with a no-argument `wait` that reaps
every child — a shell invention with no syscall behind it, needing a second
return convention to say which child did what. DG declined to invent it:

> I don't want to invent a new job control. I think it makes more sense to
> implement `wait(2)`/`waitpid(2)` behaviors.

So `wait` is now an int intrinsic of arity 0 or 1, returning the reaped pid
or `-1`, never aborting. `wait` is `wait(2)`; `wait PID` is `waitpid(2)`.
DD#126.

## The divergence that was two branches, and is now none

`notes/tasks/bug-wait-backends-diverge-no-children.md`, filed two sessions
ago: after a childless `wait`, native left `$?` at 127 and `--emit-c` set it
to 0. Two backends, two independent decisions about one edge case, and no
spec that read `$?` after a `wait` in both modes.

The obvious fix is to make the two branches agree. The one taken removes
them. `sash_wait_any(pid, status_out)` writes `*status_out` **only on a
successful reap**; both backends seed the slot with the current `$?` before
the call and write it back unconditionally. "A reap that found nothing says
nothing about any child" is then a property of the ABI, and neither backend
contains a line that could disagree with the other.

That is the same move as the parser desugar in 483 — the invariant stops
being a thing two places must remember and becomes a thing neither place can
express.

They do still share one convention, `128 + signum`, so `reaped_status` was
lifted out of `sash_wait` and both call it. `sash_wait` itself is otherwise
untouched: the pipeline waiter still wants exactly what it did.

## `-` and `$?` talk about different things, for the first time

SB5012's rule is "`-` means I am not handling this status, so `$(status)` is
unavailable afterwards". Sound for every command — until `wait`, whose value
is a **pid**. Discarding the pid says nothing about the status, and
`$(status)` is the only way to reach the child's exit at all:

```sh
- /bin/sh -c "exit 3" &
- wait
int s=$(status)      # 3
```

Without an exception, DD#126's entire status story is unexpressible. So the
state machine carries one, `stmt_reaps_child`. It is the first and the rule
still holds everywhere else, which is the argument for an exception rather
than a weakening.

## A bare `wait` is SB4016, decided rather than asked

DG wrote "`-` wait for I-don't-care". That phrase only has content if the
bare form is *not* also the I-don't-care form — otherwise `-` is decoration.
And sash already has exactly one rule about an unconsumed value: SB4016,
which rejects a bare call to an `-> int` function. Measured it on
`fn f -> int { return 1 }` + bare `f`, found the message, and applied it.
Source of truth, not a fork — step 1 of the decision tree.

There is deliberately **no condition escape**. The int-function case has one
because a function's int can be a meaningful truth value; a pid cannot, so
`if wait { … }` would read "the reaped pid was zero".

## Three positions, one arity table

`wait` is the first intrinsic with a *range* of arities, and the shape that
invites is a second list beside the fixed ones — which is precisely how
`$(( (strlen a b) ))` stayed silent while `$(strlen a b)` was SB4045.
`value_intrinsic_arity` now returns `RangeInclusive<usize>` and all three
positions read it:

- `$(wait 1 2)` — `check_cmdsub_arity`, already there;
- `$(( (wait 1 2) ))` — `arith_intrinsic_complaint`, which had the arity
  hard-coded to `len`/`strlen` and is now generalised off the table. That
  **newly catches `(getpid x)`**, which previously fell into the user-int-func
  path and yielded a silent 0;
- `- wait 1 2` — the statement position, which neither value check can see.
  It discarded the surplus in silence. New: `wait_arity_diag`, in the same
  per-command statement walk as `check_match_arity`.

## Two operand semantics for one node

`(len a)` takes a collection **name**; `(wait p)` takes a **value**. The
shared `reconstruct_intrinsic_command` turns `ArithExpr::Var("p")` into the
literal `p`, which is right for the first and wrong for the second, so
`try_arith_wait` runs before it and lowers the operand with `arith_to_ssa`.
The word form `$(wait $p)` lowers with `expr_to_value`. Both produce the one
`SsaValue::CmdSubst { argv: ["wait", pid?] }`, and both backends lower
`argv[1]` with their int lowering — so the four spellings meet at one node
rather than at four.

## The gap, reported not closed

An arithmetic *condition* sets `$?` from its own verdict. Measured:
`(( 0 )); int x=1; if (( x > 0 )) { int s=$(status) }` gives `s=0`, not 1. So
in `while (( wait > 0 )) { … }` the body cannot read the reaped child's
status. The usable shapes are `int p=$(wait); int s=$(status)` and `- wait` +
`$(status)`.

That matches wait(2), where the status is an out-param you ask for
separately, so it is arguably not a gap at all. Closing it would mean a
`waitstatus` intrinsic — purely additive, so recorded rather than guessed at.

## Gates

Sweep 7965, predicted exactly (7925 + 20 specs × 2 modes), 0 failing, 0
flips, 40 new ids. `mk test` green. Both collision checks clean, plan-vs-chk
clean.

The complexity gate caught `funccall_to_ssa` at cog 6 against a 3× bar of
5.23 — the three lines I added to it. Extracting `try_arith_wait` and moving
the `args.is_empty()` guard into `resolve_nullary_sexpr` (whose own name
asks that question) took it to cc 5 / cog 3. The gate working as designed:
grandfather-on-modify meant touching a function obliged me to bring it down.

Five existing specs changed form: three bare `wait`s became `- wait`
(`commands.sh` 14.3, `errors.sh` 55.16, `misc.sh` 38.4), `isink.11` and
`55.19` likewise, and `csc.14` — "`wait` as the sole body of a capture" —
asserted an empty string that no longer exists, so it now pins the SB4030
that a `str` target earns.
