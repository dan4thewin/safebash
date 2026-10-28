---
title: "Question: a stage applies its redirects before evaluating its arguments; a statement after"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules (a) or (b) below, and the ruling is recorded in DD55 or DD122"
  - "stgarg.33-36 are re-pinned to the ruling in both backends"
---

## What

A pipeline stage applies its own redirects and then evaluates its argument
captures. A statement evaluates its arguments first and then applies its
redirects. Since F2 (B81, journal 539) a command stage's captures run inside
the stage child, so a command stage now does what a function stage already
did. The statement order is unchanged.

The difference is visible when a capture reads stdin or writes stderr:

| Id | Program | Result |
|---|---|---|
| stgarg.33 | `: \| /bin/echo "got=$(/bin/cat)" < ro.in` (--no-taint) | `got=abc`: the capture reads the stage's `<` |
| stgarg.34 | `fn f { echo "f:$1" }` ⏎ `: \| f "$(/bin/cat)" < ro.in` | `f:abc` (control: the function stage always did this) |
| stgarg.35 | `fn g { - /bin/ls /nonexistent-stgarg35; echo x }` ⏎ `/bin/echo "a=$(g)" 2>/dev/null \| /bin/cat` | `a=x`, empty runtime stderr: the capture's error goes to the stage's `2>` |
| stgarg.36 | the same without the pipe: `/bin/echo "a=$(g)" 2>/dev/null` | `a=x`, and runtime stderr holds the ls error: the capture ran before the redirect |

## Options

- (a) Keep. A stage's I/O is set up before it runs anything, so everything
  the stage does, its arguments included, sees the stage's redirects.
- (b) Statement order in both forms, so `cmd "$(x)" 2>f` means the same
  thing whether or not it is a stage.

No DD settles it. DD55 and DD122 describe the stage child, not the order.

## Pointers

- spec/pipe_functions.sh, the `# Order.` block (stgarg.33-36).
- journal 539; notes/tasks/batch-531-findings.md, B81.
