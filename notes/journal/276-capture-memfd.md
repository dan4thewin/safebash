# 276 — the capture sink stops being a pipe

## The question that produced the answer

The deadlock note ended with four options and a recommendation (spill to a
temp file). DG did not pick from the list. He asked a different question:

> should just write to a buffer - don't we have optimizations for pipes to
> be replaced by buffers when both sides are non-external?

Two things came out of that.

**We do not have such an optimization.** I searched for one — nothing in
`notes/decisions/`, nothing in the codebase. The instinct was right and the
machinery was imagined.

**The precondition would not have covered the bug anyway.** "Both sides
non-external" describes the case the filing described: a function body
writing to fd 1 in-process. So I measured the other case:

```sh
fn emit { /bin/cat 200kb.txt; }
str x=$(emit)
```

It hangs too — rc 124 against the hang guard. A forked child inherits fd 1
and blocks on the same 64 KiB wall. The deadlock is not about who writes.
It is about **nobody reading**, and the reader cannot start until the body
returns. The filing had named the symptom it happened to trip over.

## What "write to a buffer" should mean

The literal reading — a userspace byte vector, `sash_write(1, …)` appends to
it instead of calling `write` — is the fastest possible version and is
wrong twice over:

- **A forked child cannot reach a userspace buffer.** It has an fd and
  nothing else. `cap.8` is exactly this.
- **It breaks redirects.** `echo hidden > f` inside a captured body compiles
  to save fd 1, `dup2` the file onto 1, write, restore. A buffer keyed on
  *the number 1* has no idea fd 1 was temporarily rebound; it swallows the
  write and the file is never created. Nothing else in the program would
  notice. That is `cap.10`, which passes on both sides of this change and
  exists only to make the trap visible if anyone tries the buffer later.

Both failures come from the same mistake: treating "fd 1" as a name for the
capture sink when it is a name for *whatever fd 1 currently is*.

So the sink has to stay an fd. The requirement is narrower than it first
looks — not "replace the pipe with a buffer" but **"keep the fd, lose the
blocking"**. A pipe blocks because it is a bounded queue with no reader. A
regular file never blocks; it just grows.

`memfd_create` is a regular file with no filesystem behind it. Anonymous,
in-memory, no path, no `$TMPDIR`, nothing to clean up on the abort paths,
freed when the last fd closes. It is DG's buffer — the kernel holds it, and
keeping it an fd is what lets `dup2`, redirects, and forked children go on
working untouched.

This is also what makes option B from the filing unnecessary. B's whole
cost was the temp path and its cleanup, and `memfd` has neither.

## The change

Entirely inside `sash-rt`. `sash_capture_start` swaps `pipe()` for
`memfd_create`; `sash_capture_end` seeks to 0 before draining. The returned
pair is still `(saved_stdout, handle)`, so the SSA instructions, the
Cranelift signatures, and `emit_c` are all untouched — and c-mode picked the
fix up for free.

`dup2` is doing quiet work here. `cap` and fd 1 end up sharing one *file
description*, so they share an offset: a child's writes advance the same
cursor the parent's do, and the interleaving is correct by construction
rather than by arrangement. Two independent handles would have silently
overwritten each other. `cap.11` pins it.

## Measured

| case                                   | before        | after    | bash     |
|----------------------------------------|---------------|----------|----------|
| in-process, ~500 KB (`cap.7`)          | hang (rc 124) | `508889` | `508889` |
| external in body, 100 KB (`cap.8`)     | hang (rc 124) | `100000` | `100000` |
| redirect inside body (`cap.10`)        | correct       | correct  | correct  |
| interleaved in-process/forked (`cap.11`)| correct      | correct  | correct  |

Sweep 4095 passing, 29 skipped, 0 failing, zero flips, +12 specs.

## The price, stated out loud

The sink is a regular file now, so a body that probes its own stdout sees
one:

```sh
fn emit { if [[ -p /dev/stdout ]]; then echo PIPE; else echo NOTPIPE; fi; }
str x=$(emit)
```

bash says `PIPE`. sash said `PIPE` before this change and says `NOTPIPE`
after. I measured both sides rather than reasoning about them, because a
claim about what the old build did is worth exactly as much as the run that
produced it.

That is a genuine divergence from bash, and the project's north star is
byte-identical output, so it is not mine to wave away. It is narrow —
`-t 1` still agrees (`NOTTY` both), and it takes a body deliberately
probing `/dev/stdout` to see it — and the thing it buys is the removal of
an unconditional hang. But narrow is not zero. `cap.12` pins the current
answer so that if DG reverses the call, a spec flips and says so, instead
of the delta quietly becoming folklore.

The options that keep the pipe do not survive contact:

- **A larger pipe** (`F_SETPIPE_SZ`) moves the wall to 1 MB and leaves it a
  wall.
- **Forking the body** is bash-exact including `-p`, but a forked body's
  assignments stop being visible to the caller — a language-surface change,
  not a bug fix.

## Found on the way out

`cap.10`'s sibling test turned up a **separate pre-existing bug**: nested
function capture. `h() { i=$(g); … }; o=$(h)` fails with `sash: read
failed`. It reproduces identically on the pipe runtime, so it is not from
this change. Cause: codegen holds the capture handles in a *single* pair of
slots — `capture_saved_var`/`capture_pipe_var` natively, `__cap_saved`/
`__cap_pipe` in C — so the inner capture overwrites the outer's and the
outer `CaptureEnd` reads a closed fd. Filed as
`bug-nested-function-capture-clobbers-handles`.

Worth noting how it surfaced: not from looking for it, but because a test
written to guard *something else* happened to nest two captures. The
non-regression guards keep earning their place by failing for reasons they
were not written to catch.

## What the filing got wrong

Third time in a row a filing undersold its own bug, so it is worth naming
the pattern rather than the instance. The note said the deadlock was about
a function body writing in-process. It was about nobody reading. Those
produce the same symptom on the case that was measured, and different
scopes for the fix — an in-process-only framing leads straight to the
userspace buffer, which is the one answer that does not work.

The filing was written by measuring one reproduction and describing it. The
fix needed the *class*. Reproducing is necessary and is not sufficient; the
next question after "does it reproduce" is "what else is in this class",
and `cap.8` is what that question costs when nobody asks it.
