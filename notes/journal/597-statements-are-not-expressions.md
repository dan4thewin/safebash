title: Statements are not expressions
date: 2026-11-09

## DG's ruling

After settling conditional-block sugar, the next question was whether
an `if` or loop should itself supply a result. The recorded examples
included testing a loop's stopping condition as the answer of the loop.

The AI recommended no result for control-flow statements, applying
the refusal at actual consumers rather than banning every compound
beside a logical connector. An unconsumed final action and the new
block sugar would remain. Alternatives were defined results for all
compounds, or only for `if`, including a rule for paths where no body
runs.

DG answered:

> reject; sash is not a lisp.  there's a clear distinction between an
> expression context and a statement context. I think this is valuable
> because it necessitates a procedural approach that is easy to reason
> about.

## The condition answers; the statement acts

`if`, `while`, `until` and `for` have no value or status result.
Neither the final condition nor the final body command becomes an
implicit answer, and a skipped body does not manufacture zero.
This applies to brace and keyword spellings, C-style `for`, and the
conditional-block shorthands recorded in journal 596.

Reject consuming such a result in a condition, through `!`, in a
status return or to decide a logical continuation. Wrappers and
placement do not create a result. The old question's "either operand"
wording was too broad: DD74 already distinguishes a consumed clause
from an unconsumed statement tail. Thus the historical
`cmd && if ...` final action remains valid where otherwise legal;
putting that tail in a consumed condition is different.

This does not erase the condition's own status in its handling branch,
catch failures in the body, or introduce a new status permission after
the construct. An explicit `return` inside the body still returns from
the enclosing function. A function can perform procedural work and
return its chosen result; an implicit compound result is not needed.

The same distinction preserves control flow inside ordinary stdout
captures and reader pipelines. Those enclosing operations have their
own output/aggregate contracts; their bodies do not become expressions
merely because the enclosing operation supplies a result.

## Recording and queued implementation

DD68 is authoritative; DD55/DD70/DD74/DD95 and GUIDE now agree.
The existing P3 `q-compound-as-condition` keeps its slug and becomes
implementation work. The group-retirement owner remains separate,
and the nested-status task stays open for still-legal bodies.
General-group refusals and consumed-compound refusals must not be
reported as repairs to a body's flow checks.

Implementation must take a current fixture census, preserve the
unconsumed-tail controls, use fresh ids for changed contracts such as
bng.24, and prove the consumer boundaries through both backends.
No new diagnostic or lowering architecture is chosen in this recording.

No compiler, runtime, spec or build code changed. The historical
tables were not rerun; they are not a fresh impact count or gate
prediction. No new behavior probe, sweep or coverage run was made.
Journal 577 remains the accepted implementation baseline.
