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

The reaped child's exit status lands in the **`$(waitstatus)` latch**, not in
`$?` — see the amendment below. On `-1` the latch holds `-1`.

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

### `-` discards the pid

`wait` is the first command in sash whose value is not a status, so `-` here
discards the *pid*. The child's exit status is a separate output and has a
separate reader (`$(waitstatus)`), so `-` keeps its ordinary meaning and
SB5012 keeps its ordinary rule — no exception.

```sh
- /bin/sh -c "exit 3" &
- wait
int s=$(waitstatus)   # 3
int q=$(status)       # SB5012 — `-` means `-`
```

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
again. (`notes/tasks/bug-wait-backends-diverge-no-children.md`.) The
amendment below moves the status out of the out-param entirely, which removes
even the seed/write-back pair: the runtime owns the value and both backends
call one function to read it.

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

## Amendment — §2: `$(waitstatus)` and the one-statement latch rule

Settled with DG (2026-10-09), in the same conversation, after the gap below
turned out to be load-bearing rather than cosmetic.

> go with the recommendation; be sure we are keeping the long spellings,
> `status` and `getcpid`, as canonical, with the `$?` and `$!` as sugar
>
> — DG

### The gap this closes

An arithmetic *condition* sets `$?` from its own verdict, so

```sh
while (( wait > 0 )) { int s=$(status) }
```

could not read the reaped child's status — `$?` held the condition's. That is
the drain loop DD#126 exists for, so the gap was the feature.

### The rule

**`wait` does not touch `$?` at all.** The reaped child's status goes to a
latch read by a new nullary int intrinsic, `$(waitstatus)`. On a reap it is
the child's status; on `-1` (ECHILD or any error) it is **`-1`**.

`-1` rather than "leave the previous value alone": a stale status is a wrong
answer wearing the clothes of a real one, and no child exit status can be
negative, so `-1` is a knowable one.

### The latch is readable for exactly one statement — SB5094

`$(waitstatus)` and `$(getcpid)` (sugar: `$!`) are **single-slot and silently
overwritten**. A read one statement late is not an error at runtime; it is a
different, plausible number. So the validator requires the read to be in the
statement **immediately following** the one that wrote it:

```sh
- /bin/sleep 1 &
int p=$!              # ok
```
```sh
- /bin/sleep 1 &
echo hi
int p=$!              # SB5094
```

This retires `notes/tasks/bug-bg-pid-slot-unavailable.md`, where `$!` read
before any `&` compiled and yielded an empty string.

The drain loop works because a `while` body is seeded with the state the
condition left, so the condition's reap is fresh for the body's first
statement — no special case.

Reads are checked against the *incoming* latch, before the statement's own
transition, so `- wait $!` reads a fresh `$!` and then latches the status it
reaped. A function body is a fresh context: it cannot see its caller's latch.
A `{ … }` group is braces, not a catching construct, so the latch crosses it;
`if`/`while`/`for`/`case` and `&&`/`||` do not.

### The spellings

`status`, `getcpid` and `waitstatus` are **canonical**; `$?` and `$!` are
parser-level punctuation sugar for the first two (DD#111). The diagnostics
name the long form, as SB5012's message already did.

### What this changes about program exit status

An uncaught background failure no longer colours the parent's exit code:

```sh
/nonexistent_binary_xyz &
- wait                     # exits 0 now; used to exit 127
```

That 127 was a side effect of the SB5012 exception — `- wait` left the
reaped status *available*, and an available-but-unconsumed status at the end
of a program is the program's exit status. With the exception gone so is the
accident. The author wrote `-` and read no latch; they said "I do not care"
twice. The spelling that propagates it is explicit:

```sh
/nonexistent_binary_xyz &
- wait
int s=$(waitstatus)
exit $s                    # 127
```

(`spec/errors.sh` 55.16, `spec/commands.sh` 126.36.)

### Implementation: the runtime owns the latch

`sash-rt` holds `SASH_WAIT_STATUS`, written by `sash_wait_any` on **every**
call, and read by `sash_wait_status()`. The compiler threads nothing: there is
no second ambient Cranelift `Variable` (`exit_status_var` alone has 247
mentions), no new `VarSlot` to special-case in `collect_base_vars` /
`declare_inst_bases` / `map_versioned_names` and mirror in `emit_c.rs`. Both
backends call the same function, so they cannot disagree — the same argument
that closed the childless-wait divergence, applied once more.

### Two bugs found implementing it

**A `(( … ))` *statement* was invisible to SB5012.** Its children hang off an
`ArithExpr`, and `Statement::direct_exprs` yields nothing at all for
`StmtKind::ArithTest` — deliberately, so the passes wanting `ArithExpr` nodes
walk the tree themselves. So `if (( (status) == 0 )) { … }` read an
unavailable status and raised **nothing**, while the `$(( … ))` *expression*
form was correctly rejected. `arith_test_names_intrinsic` is the shared
answer, used by SB5012, SB5094 and the reap predicate alike (126.34, 126.35).

**`- cmd &` is `Suppressed(List(Background, …))`** — the suppression wraps the
whole list — so a latch transition written into the list arm of the walker
never fires. The producer is structural (`latch_after`), one authority for
every statement shape.

## Specs

`spec/commands.sh` `126.1`-`126.20` (§1) and `126.21`-`126.36` (§2). Arity in
all three positions (statement, `$( )`, sexpr) is checked off one table,
`ssa::value_intrinsic_arity`, which now carries ranges so `wait`'s 0-or-1
lives beside the fixed arities rather than in a second list.
