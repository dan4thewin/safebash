---
title: "Question: when attached redirects take effect relative to argument evaluation"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG chooses attached-redirect timing, and the ruling is recorded in DD55 or DD122"
  - "stgarg.33-36 are re-pinned to the ruling in both backends"
---

## What

DG's 2026-11-06 left-to-right ruling (DD86, journal 580) settles the order
of word/component evaluation, including captures. This task's remaining
question is the phase at which stage I/O is installed relative to that
evaluation, not another choice about capture hoisting within the words.

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
- (b) Arguments first in both forms: evaluate arguments, then install
  the command's redirects. This retains standalone-command timing and
  changes stages.
- (c) I/O-first in both forms: install attached redirects before argument
  evaluation in standalone commands too. Argument captures then see the
  command's own redirected stdin/stderr in either context.

The consistency options change different existing programs: (b) changes
stages, while (c) changes standalone commands. The chosen rule must account
for redirect-target evaluation as well as descriptor installation, preserve
DD86's value-evaluation order and keep argument captures in the stage child.
Pipeline plumbing is distinct from the timing of the command's attached
redirects. No option is chosen.

No DD settles it. DD55 and DD122 describe the stage child, not the order.

## Pointers

- spec/pipe_functions.sh, the `# Order.` block (stgarg.33-36).
- journal 539; notes/tasks/batch-531-findings.md, B81.
