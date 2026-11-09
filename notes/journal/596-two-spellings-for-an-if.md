title: Two spellings for an if
date: 2026-11-09

## DG's ruling

DG had already retired general command groups, keeping
`cmd || { a; b }` as sugar for `if ! cmd { a; b }`
(2026-10-30, `retire-command-groups`). The remaining question was the
symmetric success-path spelling.

The AI recommended keeping `cmd && { a; b }` as exactly
`if cmd { a; b }`, with ordinary `if` scope and checking rather than
a general command group. The alternative was to reject it and require
explicit `if` for the success path.

DG answered:

> yes

## One construct, both shorthands

```sh
cmd && { a; b }          # if cmd { a; b }
cmd || { a; b }          # if ! cmd { a; b }
```

The right-hand block is an `if` body. General source groups remain
retired, including negated groups, group return producers and group
stages or redirected/background groups. Existing function, capture,
control-flow and dispatch bodies are not removed.

The corresponding `if` determines binding scope, taint/attest flow,
status/latch values and availability, and control transfers. That
includes the negation in the failure-block spelling: there is no
separate raw-status exception for a braced `||` body.
Condition handling does not catch ordinary failures in its body,
including the final command, or reach into operands/callees.

The body's lists are separate from the guard, so
`cmd && { other || recover }` follows the explicit `if` form.
DD95 still rejects actually mixed lists; no command-list grouping
construct is restored. The separate question about consuming an
`if` or loop's status is not decided by this shorthand ruling.

## Recording and queued implementation

DD55/DD68/DD70/DD95 and GUIDE now agree. The existing P3
`retire-command-groups` changes from an open decision to open work;
there is no new competing implementation owner or diagnostic allocation.
Its old diagnostic-wording questions are implementation work, not new
choices about whether retired groups remain legal.

The group cases in `q-compound-as-condition`, Q3 of
`q-statement-form-residuals`, and the nested-status task now point to
the settled boundary. Completed historical group/condition/backend
tasks retain their results without advertising group syntax as an
ongoing permission. In particular, function-stage support and legal
attest-diversion behavior must survive the source retirement.

Implementation must measure current fixtures, preserve still-legal
body assertions, allocate fresh ids for changed contracts and prove
both group refusal and `if` equivalence. A syntax error is not evidence
that a nested-flow defect was fixed. The recorded old inventories and
probes were not rerun and are not gate predictions.

No compiler, runtime, spec or build code changed. No new behavior probe,
sweep or coverage run was made. Journal 577 remains the accepted
implementation baseline; this commit records policy and backlog only.
