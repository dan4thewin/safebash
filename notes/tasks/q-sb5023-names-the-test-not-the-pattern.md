---
title: "Question: SB5023 in a `[[ ]]` test names the `[[`, not the pattern that failed — report the pattern?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the question below; the ruling is recorded as an addendum to DD#39, whose 2026-10-25 addendum states today's positions"
  - "if the ruling moves a position, the specs that pin today's (39.40–39.44 and 39.49–39.52, spec/regex.sh) are updated, and each moved form gets a spec with two bad patterns, each reported at its own"
---

## What

DD#39's addendum of 2026-10-25 made SB5023 cover every static regex the
runtime compiles. It states where each form reports it:

- an operand of `sub`, `gsub`, `match` or `matchall`, and each pattern of a
  regex dispatch, report the pattern;
- a `[[ ]]` test reports its `[[`, and an attest predicate its `attest`;
- the `${…:s/…}` sugar reports the word that holds it, and the `x =~ s/…`
  statement form its statement.

The `[[ ]]` and statement positions are the ones base (1309385) already
gave for the forms it checked: `[[ $x =~ ( ]]` at 2:1 alone and at 2:4
under `if`, and `x =~ s/(/z/` at 2:1. F15 kept them.

When one test or one chain holds two patterns, the position does not say
which one failed, and the message quotes the engine's complaint, not the
pattern. Measured natively after F15, and the batch-531 branch gives the
same positions; every program has `str x=abc` as line 1. On base all six
compiled, and the error surfaced only at run time as `regexp error`.

| Program (line 2) | SB5023 at | The bad pattern |
|---|---|---|
| `[[ $x =~ ( \|\| $x =~ a) ]] && echo m` | 2:1 twice: "missing operand for (", then "unmatched right paren" | 2:10 and 2:21 |
| `[[ $x =~ a && $x =~ ( ]] && echo m` (39.49) | 2:1 | 2:21 |
| `while [[ $x =~ a && $x =~ ( ]] { x=b }` | 2:7 | 2:27 |
| `if [[ $x == q ]] { echo q } elif [[ $x =~ a && $x =~ ( ]] { echo m }` | 2:34, the `elif`'s `[[` | 2:54 |
| `echo "${x:s/a/1/,s/c)/2/g}"` (39.41) | 2:6, the word | 2:20, the second edit |
| `x =~ s/a/1/,s/c)/2/` (39.42) | 2:1, the statement | 2:15, the second edit |

## Cause

`check_test_constructs` (validate.rs) hands `check_regex_literal` the
command's span, and `check_regex_literal` uses it for every `=~` it finds,
though each operand (`right`) has a span of its own.
`check_static_regex_operand` and `check_dispatch_regex_patterns` pass the
pattern's span. The sugar and the statement form desugar to `sub`/`gsub` and
are checked as operands, but the RE they hand the operand check carries the
word's or the statement's span.

## Question

- **(a) Keep today's positions.** DD#39's addendum stands as written, and a
  test's diagnostic names its test, as base's did.
- **(b) Report the pattern in every form.** `check_regex_literal` would
  report each operand's own span; whether that span is exact has not been
  measured. The sugar and the statement form need each RE field to carry a
  span of its own out of the desugar, which is the larger change.

Either way the cost in specs is F15's own. No earlier spec pins an SB5023
position: cov.4, 39.13, 39.16, 118.23 and rxcompile.1 assert the code (and
rxcompile.1 its text), and 39.5 and 39.8 assert only a nonzero exit with
no output.

Recommendation (made during F15, not yet ruled): (b). F13 ported positions
first so that later specs would pin final ones, and a position that cannot
tell two patterns apart is not final.

Found during the batch-531 port (step F15). Its discussion had said SB5023
would be reported at the regex operand's own span; that holds for operands
and dispatch arms only. Filed under DG's rule for the port that questions
and uncovered cases become backlog tasks.
