---
title: "Retire command groups; keep `&&` and `||` blocks as `if` sugar"
status: open
priority: P3
kind: work
depends: []
gates:
  - "implement DD55/DD68/DD70/DD95's accepted conditional-block sugar and general-group retirement in native and C"
  - "both shorthands match their explicit `if` counterparts for evaluation, scope, checking, status/latches, taint flow and control transfers"
  - "retired source groups reject through marks and all former placements, with advice for ordinary statements, an `if` or a function as appropriate"
  - "take a current parser-derived fixture census; migrate changed contracts to fresh global ids without losing coverage of still-legal bodies"
  - "before-binary and targeted reversion controls independently pin the refusals and sugar equivalence; the predicted ordinary gate passes"
---

DG, 2026-10-30 (`q-suppression-on-user-functions`, "Rulings, 2026-10-30"):

> a multi-statement group is essentially a user function without a name -
> I don't think I see any reason to support the groups. the foo || { a; b }
> can be allowed as a sugaring of if ! foo { a; b }.

DG accepted the symmetric `cmd && { a; b }` shorthand on 2026-11-09.
DD55/DD68/DD70/DD95 and journal 596 record the combined policy. The
decision is settled; compiler implementation remains open in this task.

## Accepted rule

```sh
cmd && { a; b }          # exactly: if cmd { a; b }
cmd || { a; b }          # exactly: if ! cmd { a; b }
```

The right-hand braces open the corresponding `if` body, not a general
command group. Use ordinary `if` scope and checking, without a new
function or process boundary. The condition runs once and only the
selected body runs. Body failures retain ordinary implicit checks,
including on the body's last command; condition handling does not
reach into operands or called bodies.

Status values and availability, DD126 latches, taint/attest flow,
bindings and `return`/`break`/`continue` must match that `if`. In
particular, `||` sugar inherits the negated condition's status behavior;
it has no special raw-status rule merely because the source uses `||`.
There is no new status permission after the construct.

General source groups reject: standalone, under `!` or `-`, as return
producers, stages, background jobs or redirected commands, including
inside other bodies. No wrapper or placement revives them. Ordinary
function, capture, control-flow and dispatch bodies remain legal under
their own rules. A literal quoted brace is not a group.

DD95 still rejects mixing `&&` and `||` in one list. The body's own
lists are separate, so `cmd && { other || recover }` follows
`if cmd { other || recover }`, not mixed-list grouping. Header braces
still delimit the body under DD68. Do not make a block an independent
status producer. DD68's 2026-11-09 ruling now makes every `if`/loop a
resultless statement; `q-compound-as-condition` owns enforcement.
Coordinate the two changes so a consumed shorthand cannot retain a
group's old status, while an unconsumed final action stays legal.

The diagnostic code and internal representation are implementation
work, not unresolved language choices. Advise inlining ordinary
statements or naming a function as appropriate; a former group stage
or redirected group needs a function, not another group spelling.
Do not delete internal body/carrier machinery merely because source
groups go: functions, readers and parser-generated arithmetic/loop
sequences still need their existing semantics.

## Historical inventory (2026-10-30, not remeasured)

The recorded `parse_group` (parser.rs:1365–1380) accepted a `{ … }`
group in five places:

- as a statement: bc.g4–bc.g8 (brace.sh:180–207);
- after `||` or `&&`: 55.51–55.56 (errors.sh:1475–1502) and 18.8
  (functions.sh:85);
- as a pipeline stage (`c-compound-pipeline-stage`);
- under `-`: p518.22/.23 and csfx.17. A `-` never prefixes a compound
  any more, which is `retire-suppression-on-compounds`;
- with a redirect: csfx.17 (`compound-redirects`).

Journal 453 has the history of the brace token.

## Implementation evidence

Take the fixture census when implementation starts, from the parser
rather than a text grep. A grep for `|| {` / `&& {` also matches dispatch
bodies; it found 88 lines in 13 files on 2026-10-30, 41 in dispatch.sh.
Those are historical counts, not a current migration or gate prediction.

Pin both shorthands against their explicit `if` forms: success/failure
selection, skipped effects, operand/body failure boundaries, status
reads and control transfers. Preserve legal attest-diversion behavior
and still-required refusals. A new group-syntax rejection is not proof
that a nested-flow defect was repaired; use a still-legal `if` form for
that evidence.

The group-only rows in `q-compound-as-condition` and Q3 of
`q-statement-form-residuals` now have a rule. The former task also has a
settled no-result rule for non-group compounds (journal 597); the latter
retains its separate separator/declaration questions.
`compound-redirects`, `c-compound-pipeline-stage`,
`block-falls-through-blind-to-group` and
`compound-condition-abort-catch` are already completed historical
records, not new implementation owners. Retire only their group-only
source permissions; preserve the still-legal function/body mechanisms
and assertions they established.

Coordinate with `retire-suppression-on-compounds` and the nested-status
flow task so an earlier syntax error does not masquerade as proof of
their independent checks. Allocate fresh ids above the database
high-water marks for changed contracts, then predict additions,
retirements, flips and totals from the actual fixture migration.
