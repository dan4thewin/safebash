title: DD#120 — The invoker's channel
status: implemented

A diagnostic emitted by the **invoking program** goes to the **invoker's
stderr**. A redirect written on a command establishes a channel for that
command's own output, and never captures the invoker's diagnostics.

## The rule

> The program sets up redirects **on behalf of** `cmd`, to direct the output
> of `cmd`. The setup belongs to the program invoking `cmd`, and is not
> intended to consume the output of the invoker. — DG, 2026-09-27

`cmd 2> errlog` captures the stderr of a **running** `cmd`. If `cmd` never
ran — the redirect could not be set up, or `execve` failed — it produced no
output, and nothing it did not produce can be captured. Anything printed in
that situation is the invoker speaking about its own failure, and belongs on
the invoker's channel.

To capture the invoker's diagnostics, redirect the invoker:

```sh
exec 2>> errorlog        # rebinds THIS program's stderr
cmd                      # now both go to errorlog
```

## What this covers

Every diagnostic sash emits is the invoker's. All of them move:

| diagnostic | emitter |
|---|---|
| redirect setup failure (`<path>: No such file…`) | parent, before the command |
| exec failure (`<cmd>: No such file…`) | the forked child, after `execve` fails |
| implicit-check abort (`<cmd>: command failed (exit N)`) | parent, after `wait` |
| `attest` gate failure | parent |
| arithmetic abort (divide by zero, overflow) | parent |
| `regcomp` failure, argv guard, OOM, read panic | parent |

Only output written by a running command — including `echo x 1>&2`, which is
a user write through fd 1 — travels through that command's redirects.

## Mechanism: a real channel, not a convention

The runtime holds the **invoker's own fds 0/1/2**, `SASH_SCOPE_SAVE`, dup'd
at program start with `F_DUPFD_CLOEXEC`. Every diagnostic writes to the
saved fd 2. This is deliberately an *object* rather than a rule applied at
each emit site:

- `cmd 2> f` rebinds fd 2 temporarily, on a child's behalf. The save is
  untouched, so every diagnostic stays on the invoker's channel
  automatically, at every site at once.
- `exec 2> f` rebinds the **invoker's own** stderr. The save is re-dup'd to
  follow (`sash_scope_bind`).

So fd 2 and its save are in **lockstep in the invoker**, and diverge exactly
for the window in which fd 2 does not mean "the invoker's stderr". That makes
it a checkable invariant rather than a convention: outside a per-command
redirect, the two name the same file description.

`F_DUPFD_CLOEXEC` is load-bearing, not hygiene. It is what makes the forked
child's exec-failure diagnostic work: the fd is **still open when `execve`
fails**, so the child can report on the invoker's channel, and is **closed
automatically when `execve` succeeds**, so a real command never inherits it.

### Why all three fds, and not just the diagnostic one

Because the *diagnostic* is not the only thing the invoker emits. A terminal
handler is the invoker's code too, and `EXIT { echo cleanup }` is an ordinary
write to fd 1 — not a diagnostic, so a diagnostic-only channel cannot rescue
it. Saving 0/1/2 as one set covers both, under one rule and one lockstep
invariant, and `sash_scope_restore` puts them back before either EXIT or ERR
is called (DD#128).

The same distinction bash draws by accident, sash now draws on purpose: an
EXIT trap follows a persistent `exec > f` and does **not** follow a
per-command `cmd > f`. That is the lockstep invariant, observed.

## Divergence from bash

This is a deliberate divergence from **every** POSIX shell — bash, dash and
busybox all route these diagnostics through the already-applied redirects.
That behaviour is emergent rather than designed: no shell decides to report
through the redirects; they write to fd 2, and fd 2 has already moved.

The tell is order dependence. Under bash, `cmd 2>capt >bad` and
`cmd >bad 2>capt` put the message in **different places**, a distinction no
user intends. Under DD#120 both report to the invoker's stderr, and the
order dependence disappears — which is the sign it was a symptom of the
category error rather than a feature.

Compatibility cost, accepted: a converted script doing `cmd 2>> errlog` no
longer finds setup or exec failures in the log. Use `exec 2>> errlog`.

The divergence is smaller in practice than it sounds, because sash already
differs here:

- a failure **caught** by `if`/`&&`/`||` is silent (DD#55, specs 55.25/55.50)
  where bash reports;
- an uncaught failure **aborts** where bash continues, so the message is a
  death notice rather than an advisory;
- command existence is checked at **compile time** (DD#23), so a runtime exec
  failure is a rare and graver event than bash's `command not found` — all the
  more reason it must not be swallowed into a child's log.

## Consequence: restore at terminal entry, not each abort site

An uncaught failure enters ERR; orderly termination enters EXIT (DD#128).
Both run as the **invoker**, so neither may inherit an interrupted command's
redirects.

The old END implementation had a live bug in both backends: an abort
terminated the block before `emit_restore`, leaving restoration on an
unreachable edge. Its cleanup echo went into `out.txt` after a failing
redirected command. The corresponding current program is:

```sh
ERR { - echo cleanup }
/bin/false > out.txt
```

Its cleanup belongs on the invoker's stdout. Three old abort paths
leaked — nonzero-status abort, exec failure, and `echo` write failure — while
the setup-failure path was correct only because it happened to restore
explicitly.

The fix is the same object, not a fourth patch: `sash_scope_restore()` runs
before the selected generated callback, the common runtime dispatch point. A
future abort site is then correct by construction rather than by remembering
to restore, which is precisely the discipline this bug proves gets forgotten
— both backends had it wrong, identically, so the differential oracle could
not see it.
