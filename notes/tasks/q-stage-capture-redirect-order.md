title: "Evaluate arguments before attached redirects"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "standalone commands and legal pipeline stages evaluate and retain arguments before evaluating targets or applying their attached redirects, in both backends"
  - "redirect targets observe completed argument effects; targets and redirects run in redirect order, stopping at the first failure"
  - "stage arguments remain in the stage child after pipeline plumbing; enclosing redirects, capture routing and DD122 lexical reader ownership retain their scopes"
  - "argument failures retain their own checks and no-reach; redirect failure skips later redirects and the command without undoing completed argument effects"
  - "stgarg.33-36 are reconciled with the ruling, with fresh ids for changed contracts and focused native/C before and reversion controls"
  - "external, builtin and function command paths agree, including redirected calls and captured arguments; the predicted ordinary gate passes"

## Decision, 2026-11-07

DG: **"go with arguments first"**. DD86 records the rule; DD55, DD122,
DD130 and journal 585 state its failure/process consequences. Standalone
commands and stages use the same sequence: evaluate arguments, then
evaluate each redirect target and apply that redirect, then run the
command. The stage/standalone split and I/O-first alternative are rejected.

The slug is retained for its links. This is now an open implementation
bug, not a policy question, and no implementation is included in the
decision recording. Priority is unchanged.

DG's 2026-11-06 left-to-right ruling separately governs word/component
evaluation (`q-capture-hoisting-order`, journal 580). This task owns the
phase relative to attached redirects, not another choice about hoisting
within the words.

## Recorded evidence and required results

A pipeline stage currently applies its own redirects and then evaluates
its argument captures. A statement evaluates its arguments first. Since
F2 (B81, journal 539), a command stage's captures run inside the child,
matching the pre-existing function-stage timing.

These existing spec results were not re-run for the decision recording:

| Id | Program | Recorded result | Required result |
|---|---|---|---|
| stgarg.33 | `: \| /bin/echo "got=$(/bin/cat)" < ro.in` (--no-taint) | `got=abc`: capture reads the attached `<` | `got=`: capture reads the empty pipe instead |
| stgarg.34 | `fn f { echo "f:$1" }` ⏎ `: \| f "$(/bin/cat)" < ro.in` | `f:abc` (an F2 control) | `f:`: function arguments follow the same rule |
| stgarg.35 | `fn g { - /bin/ls /nonexistent-stgarg35; echo x }` ⏎ `/bin/echo "a=$(g)" 2>/dev/null \| /bin/cat` | `a=x`, empty runtime stderr | `a=x`, with the helper's error on inherited stderr |
| stgarg.36 | the same without the pipe: `/bin/echo "a=$(g)" 2>/dev/null` | `a=x`, helper error on inherited stderr | Unchanged |

The separate native CLI example in journal 585, run during the preceding
discussion at `75984b2e` without a taint bypass, independently confirms the
stderr split. Its integer helper writes `helper ran` to stderr and returns
7. Both outer commands print 7; only the pipeline's attached error file
receives the helper's message. Under the ruling, both messages go to
inherited stderr and both files stay empty. This was not a C measurement.

## Implementation boundaries

Keep argument evaluation in its owning body/process. In a stage, pipeline
plumbing is established before arguments; only the command's attached
redirects move after them. Do not restore the B81 parent-evaluation bug or
change DD122's parent-process readers. Captures keep their own output
channel and inner redirects; outer active redirects remain active.

Retain argument values, not merely an earlier visit to expressions whose
loads still happen after setup. Redirect targets are evaluated after the
arguments, so a global changed by an argument helper is read at its new
value by the target. Keep DD119's target admissibility checks; this ruling
does not legalize target captures or intrinsics.

Once the argument phase succeeds, evaluate each target at its redirect's
turn and apply the redirect before proceeding to the next. Stop at the
first failure. DD130 owns classification and recoverable handling:
completed argument effects and earlier file effects are not rolled back,
later redirects and the command do not run, and scoped descriptors must
be restored before continuation. A prefix or condition must not reach
into an argument capture or helper merely because its work moved.

Use the shared lowering paths so native and C agree. Keep coordination
with `q-capture-hoisting-order` and `recoverable-file-failures` explicit;
neither neighboring implementation is completed by this task.

Re-pin changed spec contracts at fresh ids above the database high-water
mark. Preserve stgarg.36's standalone control and add focused controls for
actual command-body routing, target reads after argument mutations,
stage-child isolation and failure ordering. Predict new/retired ids before
the sweep rather than carrying forward the historical F2 counts.

## Pointers

- spec/pipe_functions.sh, the `# Order.` block (stgarg.33-36).
- Journals 539 and 585; notes/tasks/batch-531-findings.md, B81.
