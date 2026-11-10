title: "Improve identification of offending static regex patterns in SB5023"
status: open
priority: P3
kind: work
depends: []
gates:
  - "When undertaking the task, evaluate diagnostic approaches, including per-pattern positions, explicit pattern text and a catalogue of unique regexes"
  - "The chosen diagnostics clearly identify offending patterns in multi-pattern tests, attest predicates and substitution chains; preserve existing operand and dispatch coverage"
  - "Preserve SB5023, the dialect and validation scope, including DD132's quiet invalid r? candidates; cover changed diagnostics with before/reversion evidence and fresh spec ids where contracts change"

## Accepted objective; approach deferred

DG accepted the quality goal on 2026-11-10:

> yes to better diagnostics, but evaluate alternate approaches like a catalog of unique regex

After the comparison, DG directed:

> defer implementation specifics to when we undertake thw task

DD39 now requires clear identification of the offending static regex,
including when several patterns share a construct. This is implementation
work, not an unanswered question about whether to improve the diagnostics.
It remains open and P3; no compiler change is claimed.

The earlier recommendation to report every pattern's own position is
one candidate, not the ruling. Evaluate it alongside explicit pattern text
and a unique-pattern catalogue when work begins. Exact locations,
representation, grouping and rendering remain undecided. This task does
not require runtime caching or a new source-level regex interface.

## Historical F15 baseline

DD#39's addendum of 2026-10-25 made SB5023 cover every static regex the
runtime compiles. Its retained baseline describes the existing positions:

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

These are the original F15 measurements, not probes rerun for the
2026-11-10 decision.

## Source context

`check_test_constructs` (validate.rs) hands `check_regex_literal` the
command's span, and `check_regex_literal` uses it for every `=~` it finds,
though each operand (`right`) has a span of its own.
`check_static_regex_operand` and `check_dispatch_regex_patterns` pass the
pattern's span. The sugar and the statement form desugar to `sub`/`gsub` and
are checked as operands, but the RE they hand the operand check carries the
word's or the statement's span.

Source inspection at `7a378603` confirms those paths. All three forms
already share `check_static_regex`; a catalogue is not needed to establish
a common regex validator. The parser already computes per-edit origins,
although some reconstructed input deliberately retains a coarse origin.
These are useful starting points for the later evaluation, not a chosen
implementation.

The old `dd39-precompile-literal-regex` task was marked done, but the
inspected runtime still calls `regcomp9` on use. That separate record has
been reopened; do not assume a reusable runtime catalogue exists or make
its implementation a prerequisite for better diagnostics.

## Coverage to revisit when implementing

F15's location assertions include 39.40-39.44 and 39.49-39.52 in
`spec/regex.sh`. Its earlier census found cov.4, 39.13, 39.16, 118.23 and
rxcompile.1 asserting the code (rxcompile.1 also its text), and 39.5 and
39.8 only a nonzero exit with no output. Recheck the live suite when work
begins rather than treating that historical inventory as a gate prediction.
New assertions must pin the selected diagnostic contract without requiring
an approach DG has left open.

Found during the batch-531 port (step F15). Its discussion had said SB5023
would be reported at the regex operand's own span; that holds for operands
and dispatch arms only. Filed under DG's rule for the port that questions
and uncovered cases become backlog tasks.
