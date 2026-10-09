---
title: DD#126 — `wait` is wait(2)/waitpid(2), returning a pid and never aborting
status: accepted
---

Settled with DG (2026-10-09), in the same conversation as DD#125.

> I don't want to invent a new job control. I think it makes more sense to
> implement `wait(2)`/`waitpid(2)` behaviors. `wait` without pid behaves like
> `wait(2)` and with pid behaves like `waitpid(2)`. Waiting for all would
> require a loop, e.g. `while (( (wait) > 0 )) { : }`.
>
> `wait` as an int-returning builtin shouldn't abort, and reading `$(status)`
> seems like the right way to consume the child's exit.
>
> — DG

## The rule

`wait` is an **int-returning intrinsic**, arity 0 or 1.

| form | syscall | value |
|---|---|---|
| `wait` | `wait(2)` | the reaped pid |
| `wait PID` | `waitpid(2)` | the reaped pid (= `PID`) |
| either, on `ECHILD` or any error | — | **`-1`** |

It **never aborts**. `-1` is the answer, not a fault — which is what makes the
drain loop expressible at all:

```sh
while (( wait > 0 )) { n=$(( n + 1 )) }
```

The reaped child's exit status lands in `$?` / `$(status)`. **On `-1` the
status is left unchanged**: a reap that found nothing says nothing about any
child.

### The value is a pid, so a bare `wait` is SB4016

`wait` yields a value, and sash has one rule about unconsumed values —
SB4016, the one that rejects a bare call to an `-> int` function. `wait` obeys
it:

```
$ echo bg &
$ wait
SB4016: `wait` reaps a child and yields its pid — capture it with `$(wait)`
        or discard it with `- wait`
```

DG's phrase "`-` wait for I-don't-care" only has content if the bare form is
*not* also the I-don't-care form, and SB4016 is the precedent that settles
which one it is.

There is **no condition escape**. A boolean context consumes a *status*, and
`wait`'s value is a pid, so `if wait { … }` would read "the reaped pid was
zero" — an accident, not an idiom. (The int-function case has an escape
because a function's int can be a meaningful truth value; a pid cannot.)

### `-` discards the pid, not the status

`wait` is the first command in sash where the two differ. Everywhere else `-`
means "I am not handling this status", and SB5012 then makes `$(status)`
unavailable. Here `-` discards the *pid*, and `$(status)` is the only way to
reach the child's exit:

```sh
- /bin/sh -c "exit 3" &
- wait
int s=$(status)      # 3
```

So the SB5012 state machine carries one exception, `stmt_reaps_child`.

### Both operand spellings work

`$(wait $p)` takes a word; `$(( (wait p) ))` takes an arithmetic expression. A
bare `wait` inside `$(( ))` is the nullary call (DD#125). All four funnel into
one `SsaValue::CmdSubst { argv: ["wait", pid?] }` and both backends lower
`argv[1]` with their int lowering.

This is why `wait` does not go through `reconstruct_intrinsic_command`, which
the other arith-sexpr intrinsics share: that builder turns `ArithExpr::Var(p)`
into the *literal* `p`, which is right for `(len a)` — a collection **name** —
and exactly wrong for `(wait p)`, where `p` is a value.

## What this replaces

`wait` used to be a statement builtin that **aborted** when it found no
children, and the two backends did not agree about it: native left `$?`
alone, the C backend wrote the failure in. The abort was what made "wait for
all" inexpressible without a flag, and the divergence is closed by the ABI
rather than by a matching pair of branches — `sash_wait_any(pid, status_out)`
overwrites `*status_out` only on a successful reap, and both backends seed
the slot with the current `$?` and write it back unconditionally. Neither
backend has a branch on the childless case, so neither can get it wrong
again. (`notes/tasks/bug-wait-backends-diverge-no-children.md`.)

The runtime entry point is new; `sash_wait` is untouched and still serves the
pipeline waiter. They share `reaped_status`, so the `128 + signum` convention
cannot drift between them.

## What was declined

**A `wait` that waits for all.** DG: "I don't want to invent a new job
control." Bash's `wait` with no argument waits for *every* child; that is a
shell invention with no syscall behind it, and it needs a second return
convention to say which child did what. The loop is one line and says what it
does.

**`waitpid(0, …)`.** `rustix::process::waitpid(None, …)` lowers to
`waitpid(0, …)` — "any child in my **process group**", not any child.
`rustix::process::wait` is the real `waitpid(-1, …)`. The old code used the
former by accident.

## Known gap

An arithmetic *condition* sets `$?` from its own verdict, so in

```sh
while (( wait > 0 )) { int s=$(status) }
```

the body cannot read the reaped child's status — `$?` is the condition's. The
usable shapes are `int p=$(wait); int s=$(status)` and `- wait` +
`$(status)`. This matches wait(2), where the status is an out-param you ask
for separately; closing it would mean a `waitstatus` intrinsic, which is
purely additive and not done here.

## Specs

`spec/commands.sh` `126.1`-`126.20`. Arity in all three positions (statement,
`$( )`, sexpr) is checked off one table, `ssa::value_intrinsic_arity`, which
now carries ranges so `wait`'s 0-or-1 lives beside the fixed arities rather
than in a second list.
