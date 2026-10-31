---
title: "Capture, save status, attest once, then branch"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DD#121 makes the intended scope of capture-condition branch adjacency explicit"
  - "Specs pin the agreed capture/status/attest/branch idiom in native and C modes"
  - "No else-adjacency change is made without a language decision"
---

# Capture, save status, attest once, then branch

Reframed 2026-10-31 after DG questioned the failure-branch attest in the
original reproducer:

> I think the second attest in the else is problematic. I think the
> supported spelling will be like: cap=$(cmd) || ok=$(status);
> attest cap [[ ... ]]; if ((ok==0)) {...} else {...}

The earlier task classified the `else` rejection as a confirmed bug and
prescribed copying the condition's adjacency seed into that branch.
That prescription is withdrawn. This task records the proposed idiom
and asks the native project to clarify the branch exception, rather than
assuming that `else` must become legal.

## Proposed spelling

Capture and save the command's status in one compound statement, attest
the captured definition unconditionally, then branch on the saved status:

```sash
int ok=0
cap=$(cmd) || ok=$(status)
attest cap [[ ... ]]
if ((ok==0)) { ... } else { ... }
```

The command and predicate above are placeholders.

`||` only assigns `ok` on failure. The explicit zero initialization makes
the success case independent of earlier attempts; reset `ok=0` before
each subsequent capture.

[DD#55](../decisions/DD55-error-model.md) permits `$(status)` as the first
expression in the handling branch. Save it there, before the attest can
replace the current status. [DD#121](../decisions/DD121-attest-adjacency.md)
measures a compound statement as one tainting operation, so the following
attest is adjacent without a special rule for a later `if`.

An uncaught attest still aborts if its predicate fails. The later branch
therefore sees validated output and the original command status, not the
attest's status.

## Current evidence

The supplied native CLI already accepts this sequence for:

- a successful single external command;
- a failed single external command;
- a failed external pipeline;
- a producer that writes `partial` and exits 7: the captured value remains
  `partial`, and the saved status remains 7 through the attest.

There is no demonstrated implementation gap in that spelling.

The original branch-local reproducer remains useful evidence of the
current boundary, not a requirement that it compile:

```sash
if captured=$(/bin/false) {
    attest captured [[ $captured == '' ]]
    echo unexpected
} else {
    attest captured [[ $captured == '' ]]
    echo caught
}
```

The first `else` attest is SB5088. Replacing `/bin/false` with
`/bin/true` demonstrates that the corresponding `then` attest is accepted;
the pipeline variant has the same `else` rejection.

## Policy to settle

DD#121's thesis says attest must immediately and unconditionally follow
the tainting operation. Its structural-position table nevertheless says
"an if condition -> its branches", with a `then`-only example.
[DD#88](../decisions/DD88-capture-condition.md) retains failure stdout
but says the captured target is meant to be used on success.

Make the intended boundary explicit: does the capture-condition exception
cover only the body attached to the condition, or also an `else` attest?
Include negated conditions in that clarification: `then` is not necessarily
the command's success path. Is the straight-line idiom above the supported
way to validate output irrespective of status? Do not broaden branch
eligibility just to make the original reproducer pass.

The conditional taint walker in `sash/src/validate.rs`, currently
`taint_if`, seeds `then` from the condition and passes an empty
`JustTainted` into `else`. The adjacency gate is currently
`check_adjacency`; compound-statement taint provenance comes from
`block_taints`. These names are orientation, not fixed implementation
addresses. Inspect the code that seeds branch entry and joins paths only
after the native project settles the policy.

## Completion

Clarify the relevant DD wording and pin the agreed behavior with focused
CLI-driven specs in the existing capture-condition/taint suite. Preserve
status magnitude, failed-producer output, unconditional predicate execution,
and genuine non-adjacency rejection. Both native and C modes must follow
the same decision.

Do not add rival coverage for claims existing specs already pin. Choose
IDs using database history as well as source collision checks, predict
new check counts, and run the ordinary native-project gates if code or
specs change. This handoff does not authorize compiler changes in the
C++ experiment checkout.
