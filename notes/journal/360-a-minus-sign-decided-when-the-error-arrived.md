---
title: A minus sign decided when the error arrived
date: 2026-09-18
---

# A minus sign decided when the error arrived

The branch queue reached `fold_arith_op`, and the cold lines there were
not one branch but three whole families: every comparison operator
(`Ne Lt Le Gt Ge`), every bitwise operator (`Shl Shr BitAnd BitOr
BitXor`), `Pow`, and both divide-by-zero arms. Sixteen arms of one
function, none of them ever folded.

The reflex is to read the arms and decide they look fine. The method
this stretch has been built on says otherwise: answer by execution.
So I wrote a matrix — nineteen operators against six operand pairs,
each expression lowered twice (folded, with literal operands; and at
runtime, through `(( x = N ))`), each of those checked against bash
*and* against each other. Two hundred and twenty-eight comparisons.

Everything matched bash except division and modulo by zero, which are
deliberate under DD#25. The folder is correct. But the cross-check —
folded against runtime, the same expression lowered two ways — is
where the interesting column was, and it was not the one I went
looking for.

## The gap

SB5077 rejects a program whose divisor the folder can prove is zero.
DD#25 states its reach precisely: "literals and operators over
literals, through parens." That is what the decision says. It is not
what the code did.

`const_eval_arith` had arms for `Literal`, `Paren` and `BinOp`, and
fell to `_ => None` for everything else. `ArithExpr::UnaryOp` was
therefore unevaluable, and a divisor containing *any* unary operator
was not provable. I probed eighteen spellings of zero:

- **caught:** `0`, `2 - 2`, `3 - 3`, `2 * 0`, `0 * 2`, `0 + 0`,
  `1 - 1`, `2 / 3`, `1 % 1`, `0 / 1`, `0 | 0`, `0 & 1`, `1 ^ 1`,
  `0 << 2`, `4 >> 3` — every binary operator.
- **missed:** `1 + -1`, `-1 + 1`, `-0` — every form containing a
  unary minus.

Fifteen spellings of one fault reported at compile time, three
reported at runtime, and the only thing separating them was a minus
sign. The spec block for SB5077 already contains a comment, written
months ago, calling that exact shape "the spelling-dependent
inconsistency SB5077 exists to remove." It had been reintroduced one
level down, in the function the rule depends on.

This was an oversight rather than a scope decision, and the function's
own documentation is what settles it: `const_eval_arith` declines
"variables, expansions, calls," and a negated literal is none of
those. DD#25's stated reach already covered it.

## Why it drifted

The fix is four lines, but the reason it was needed is the part worth
keeping. `const_eval_arith` carries a doc comment explaining that it
calls "the transform's own `fold_arith_op`, not a second copy — the
two must agree about what 'the folder proves' means, and sharing the
function is the only way to guarantee that."

That discipline was applied to the *binary* operators and not to the
unary ones. `try_fold_unop` worked on `SsaValue` strings;
`const_eval_arith` worked on `i64`. Different types, so no shared
function was possible, so none was written, so they drifted — and the
drift was invisible because one of the two simply had no unary case at
all. The principle was stated, believed, and half-implemented.

So the fix is not an extra match arm. It extracts the i64 core as
`fold_unary_op` and has both callers use it, which is what the comment
claimed was already true. `Neg` uses `checked_neg`, so `-i64::MIN`
declines to fold and defers to the runtime trap — consistent with the
constant-folding rule for overflow that DD#25 already sets out.

Probed after: the three that escaped are now SB5077; everything
previously caught still is; `!5`, `~-1`, `-(3 - 3)` and `+0` are newly
provable; and `-1`, `!0`, `~0`, `-(2 - 3)` still compile and still
produce `-5`, `5`, `-5`, `5`. The wider reach does not over-reject, and
that converse is pinned as a spec rather than left to trust.

## The collision I walked into

Sixteen specs went in for the fix and the cold fold families. The
mechanical collision check came back clean. It was wrong.

`coverage.sh` ends with a loop that assigns `cov.306`, `cov.307` and
`cov.308` to a shell variable in a `case`, then calls `chk $_id`. The
check greps for `chk <literal>`. An id that reaches `chk` through a
variable is invisible to it, so those three ids look unused — and I
numbered my new block starting at `cov.306`.

Both sets passed, so nothing reported anything. The loop runs last, so
it silently overwrote three of my specs. Three tests I had just written
to pin a bug fix were not being recorded at all, and the suite was
green either way. This is precisely the failure AGENTS.md describes for
cross-file `chk` collisions, arriving by a route the documented check
cannot see.

What found it was not the collision check. It was the arithmetic. The
file reported `ran=323` while the source contained 321 greppable `chk`
tokens, and that two-count discrepancy had no innocent explanation.
Chasing it produced the loop.

I have added the missing check — the ids assigned to `chk`-bound
variables, compared against the literal ids — and audited the whole
suite for the pattern. There is exactly one such site, with three ids.
The blind spot was real but contained.

## And the count itself was lying

Reconciling that discrepancy turned up a second measurement fault. I
had been predicting sweep totals against `SELECT COUNT(*) FROM
test_status`. That view is `tests LEFT JOIN runs`, and `tests` is a
permanent registry: a spec that is deleted keeps its row and its last
recorded status forever. I deleted one spec this session (`75.99`, an
exact duplicate of the older `cov.141`) and it went on reporting as
passing.

The prediction discipline had matched exactly on every run this
session — because every previous run only ever *added* specs. The
first deletion exposed it. The authoritative count is `run.sh`'s own
output, and the source-derived ground truth is the weighted `chk`
count across the spec files, which agrees with it.

With the renumber applied, predicted 5553 and the sweep returned 5553:
2668 c + 13 skipped + 2872 native. Zero failing.

## What this round is really about

Three findings, and only the first was the one I set out to get.

The coverage number pointed at sixteen cold arms. Those arms were
correct, and the specs I wrote for them are worth having — eleven
operators whose constant-folded form the suite had never once checked.
But the bug was not in the cold code. It was in a *live* function that
the cold code made me look at sideways.

The other two came from refusing to accept a number I could not
explain. A two-count discrepancy between `ran` and a grep is exactly
the size of thing that is easier to round off than to chase, and it
was concealing both an id collision in code I had written minutes
earlier and a systematic error in how I had been verifying every sweep
for weeks.
