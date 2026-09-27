---
title: Whose output is it
date: 2026-09-27
---

# Whose output is it

Journal 425 ended by fixing a bug so that sash matched bash: the redirect
setup-failure diagnostic goes *through* the redirects that already succeeded,
so `cmd 2> capt.txt > /bad` puts the message in `capt.txt`. Two backends, one
shared inversion, now agreed with every shell I could test.

DG asked whether the bash behaviour was genuinely defensible or another
foible. It was the right question, and the answer turned the fix inside out.

## The category error

bash, dash and busybox all behave identically here, so it is not a bashism.
But identical is not the same as designed. No shell *decides* to report
through the redirects — each writes to fd 2, and fd 2 has already moved. The
behaviour is emergent.

The tell is order dependence. Under bash, `cmd 2>capt >bad` and
`cmd >bad 2>capt` put the message in **different files**. No user intends
that distinction; it is a side effect of left-to-right application leaking
into a diagnostic that has nothing to do with the redirect list.

DG named what was actually wrong:

> the program sets up the redirects on behalf of cmd, to direct the output of
> cmd. The setup belongs to the program invoking cmd, and is not intended to
> consume the output of the invoker.

`cmd 2> errlog` captures the stderr of a **running** cmd. If cmd never ran —
the redirect could not be opened, or `execve` failed — it produced no output,
and nothing it did not produce can be captured. Whatever gets printed in that
situation is the invoker speaking about its own failure.

I proposed a boundary: setup failure is the invoker's, but exec failure could
still go through the redirects, to preserve the `cmd 2>/dev/null` idiom. DG
rejected it. An exec-failure diagnostic is the invoker's too; to capture the
invoker you redirect the invoker (`exec 2>> errlog`). He added that exec
failure is graver in sash than in bash, because command existence is checked
at compile time (DD#23) — a runtime exec failure is a rare and serious event,
all the more reason it must not be swallowed into a child's log.

That is DD#120, and it is a deliberate divergence from every POSIX shell.

## An object, not a rule

The implementation could have been a rule applied at each of the twelve
runtime emitters: "write to the invoker's stderr, not fd 2". Instead the
runtime takes a high-numbered `F_DUPFD_CLOEXEC` copy of fd 2 at startup and
every diagnostic writes there. One object, and every site is correct at once.

`F_DUPFD_CLOEXEC` is load-bearing rather than hygiene. The fd is still open
when `execve` **fails**, so the forked child can report on the invoker's
channel, and closed automatically when `execve` **succeeds**, so a real
command never inherits a stray descriptor. The child-side exec-failure
diagnostic works *because* of the flag, not merely tidily.

The payoff was visible immediately. Wiring the init into `sash_startup` —
which both backends reach through `_start`, so neither needed a codegen
change — fixed three of the four channels in one step, in both backends, with
no per-site edits:

| shape | before | after |
|---|---|---|
| `/bin/echo hi 2> f > /bad` | into `f` | invoker stderr |
| `/nonexistent 2> f` | into `f` | invoker stderr |
| `/bin/false 2> f` | into `f` | invoker stderr |
| `/bin/sh -c "echo real 1>&2" 2> f` | into `f` | into `f` (unchanged) |

## The question that generalised it

I had also found a live bug while measuring: `END { echo cleanup }` followed
by `/bin/false > out.txt` wrote `cleanup` **into `out.txt`**. The abort
terminates the block before the command's `emit_restore` can run, so the
restore sits on an unreachable edge and END inherits the redirects. Three
abort paths leaked — nonzero status, exec failure, and `echo` write failure.
Only the setup-failure path was safe, and only because it happened to restore
explicitly.

I brought DG two options: restore at each abort site, or a runtime stack of
pending fd saves. He asked instead:

> if the diag fd exists, why does END need anything else?

The literal answer is that `echo cleanup` is not a diagnostic — it is an
ordinary write to fd 1, which a fd-2 diagnostic channel cannot rescue. But
the question was pointing past that. The right move was not a second
mechanism; it was the same object, generalised.

So `SASH_DIAG_FD` became `SASH_SCOPE_SAVE[3]`: the invoker's own 0/1/2, saved
together, under one rule. `sash_scope_bind()` takes them at startup and again
after a persistent `exec` redirect; `sash_scope_restore()` puts them back on
entry to END. The diagnostic channel is now just the fd-2 member.

That is strictly better than either option I brought. The stack would have
been new state with its own lifetime; the per-site restores would have been a
rule that this very bug proves gets forgotten — both backends had it wrong,
identically, so the oracle could not see it. Restoring at END entry is one
call at the single place all three aborts unwind to, and a future abort site
is correct by construction.

## The invariant, and the control that pins it

fd 2 and its save are in **lockstep in the invoker**, diverging exactly for
the window in which fd 2 does not mean "the invoker's stderr". `exec 2> f`
rebinds both; `cmd 2> f` rebinds only fd 2, temporarily.

Bash draws the same distinction by accident, and it is worth stating that it
does, because it is what makes `sash_scope_restore` a restore rather than a
reset: an EXIT trap follows a persistent `exec > f` and does **not** follow a
per-command `cmd > f`. So END must be restored to the invoker's *current*
fds, not to the ones it started with. That is spec `iv.9`, and a
`sash_scope_restore` that reset to the startup fds passes `iv.6`-`iv.8` and
fails only there.

Every check in the new block is paired with a control, because "the file is
empty" is satisfied by a compiler that never writes to redirects at all:

- `iv.3` — a *running* command's own stderr still reaches the redirect
- `iv.5` — the same diagnostic, same fd, same file as `iv.4`; only the
  redirect's scope differs
- `iv.9` — the persistent `exec >` control above

The rewritten `rd.5`/`rd.7` assert the capture file is **present but empty**,
which is sharper than absence: it proves the `2>` really was applied, so the
message could have landed there and did not.

## Neuters

Three disjoint neuters, all in the runtime, each failing exactly its own
cohort and nothing else:

| neuter | fails |
|---|---|
| `diag_write` → raw fd 2 | `rd.5`, `rd.7`, `rd.8`, `iv.1`, `iv.2`, `iv.5` |
| `sash_scope_bind` → no-op after startup | `iv.4`, `iv.9` |
| `sash_scope_restore` → no-op | `iv.6`, `iv.7`, `iv.8` |

Predicted 6576, swept 6576, zero failing, zero flips, 18 new ids.

## What I'd carry forward

Twice now the better design came from DG asking a question rather than
choosing between the options I offered. In 425 it was "is the bash behaviour
genuinely defensible"; here it was "why does END need anything else". Both
times I had framed a local choice and both times the frame was the problem —
the first assumed compatibility was the goal, the second assumed the new
mechanism was finished.

The transferable part is the shape of the fix. A rule repeated at every site
is a bug waiting for the next site; an object that the sites cannot get wrong
is not. The diagnostic fd fixed three channels at once because it was an
object. Generalising it to the scope save set fixed three abort paths at once
for the same reason. When the choice is "apply this discipline everywhere" or
"build the thing that makes the discipline unnecessary", the second is worth
real effort — and the evidence it is worth it is that the discipline has
already been forgotten once, in both backends, identically.
