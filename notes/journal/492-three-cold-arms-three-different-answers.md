---
title: Three cold arms, three different answers
date: 2026-10-11
---

# Three cold arms, three different answers

Continuing the coverage queue. Three items came off the top of the ranking,
and no two of them wanted the same treatment — which is the whole argument for
classifying before mining.

## 1. The fallback DG's ruling had just killed

Head of the queue was `lower_cmdsubst_as_int`, 14 cold lines, and it was a
function I had edited that morning. Its tail fell back to `lower_value` plus
`sash_atoi` when a capture in integer context had no intrinsic arm, and
`epochseconds` was the documented last user of that path. Making `epochseconds`
return an `i64` — DG's denotation ruling — removed it.

So the fallback was cold *because of a change made hours earlier*. The
temptation is to delete it and move on. The repo rule says enumerate the
producer, so I did:

- `type_check`'s SB4030 rejects any capture in int context whose denotation is
  not `Int`. Measured, not assumed: `$(getcwd)`, `$(getenv …)` and an external
  command were each tried in both `int x =` and array-subscript position. All
  three first reported SB5020, because taint runs first; `--no-taint` exposed
  SB4030 behind every one of them.
- The two *stateful* Int names, `status` and `getcpid`, never reach codegen as
  captures — `nullary_stateful_slot` rewrites them to the `?` and `!` slots.
- The remaining seven have `int_intrinsic_i64` arms, and `wait` leaves by an
  early return.

Unreachable by construction, so it ICEs now rather than quietly producing an
`atoi` of whatever text showed up.

The enumeration turned up something worth recording on its own. **Three
hand-maintained lists of the same set exist** — `resolve::INTRINSICS` (nine Int
names), `resolve_nullary_sexpr`'s stateless list (seven), and
`int_intrinsic_i64`'s arms (seven plus `status`). There is already an ICE
guarding list-1-against-list-2, but it sits in `resolve_nullary_sexpr`, so it
only sees the `(name)` sexpr form. The word form `$(name)` builds its
`CmdSubst` in `handle_command_substitution` and never passes through there.
That hop is exactly what the new ICE closes, which is why it is orthogonal to
the existing one rather than a second asking of the same question. `getcpid`
is absent from the emission table today and works only because of the stateful
rewrite — fine now, and precisely the kind of thing that stops being fine
silently.

## 2. The comment whose premise a recent commit had falsified

Next was `resolve_command`, nine cold lines. Seven were already triaged by
earlier sessions with careful comments: validate-shadowed, deliberately not
ICEs. Not work.

The eighth was different. `is_external_looking` has a `name.is_empty()` arm,
and its sibling `is_resolvable_external` had the *same* arm deleted by a prior
session, on the recorded grounds that "an empty head cannot be an
`ExprKind::Literal`".

That was true when it was written. It is not true now. The SB5096 work
(journal 488) made `normalize_command_head` turn a single-quoted head into a
literal, so `''` is now a zero-length literal command name — and having no bare
spelling, it does not trip SB5096 itself. A change three commits earlier had
silently falsified a comment's premise, and nothing connected the two.

I confirmed reachability **by execution**, not by reading, and it took four
tries. `$('')` as an `if` condition, a pipeline stage, a bare head and an
assignment RHS all left the line cold. The right spelling came from reading the
call-site guard rather than guessing: `is_capture_condition_rhs` is reached
from `validate_assignment_stmt` only when `in_condition`, i.e. DD#88's
`if x=$(cmd) { … }` form. `if y=$('') { /bin/echo hi }` warms the line;
`if x=$(/bin/echo hi) { … }` does not.

A genuine spec gap, then — category 1 — created by a change that had no idea
it was creating one. Three specs (`qhead.10`–`qhead.12`), and both comments
corrected: the premise fixed in one, the arm marked live in the other with the
spec that reaches it.

The capture-condition form reports SB4025 *and* SB5011. SB4025's wording is a
poor fit for an empty name, but the statement it makes is true and SB5011 is
there to explain the real problem, so I left it. DG has not seen this.

## 3. The arm that answered silently

Last was `expr_to_string`, which a filed task already described: entered
21338 times, and the `Literal` arm is the only one ever taken. The task's
gates asked for every call site to be enumerated from the producer side.

Twenty-four sites. **Ten of them take a command head**, and one guarantee
covers all ten: `check_cmd_at` rejects a non-`Literal` head and returns. The
only hop worth checking was whether that traversal reaches *pipeline stages* —
it does, via `structural_bodies`' `Pipeline(p) => vec![&p.commands]`, which is
what covers `push_command_stage`. Of the rest, two take `signal`'s arguments
(SB4059–SB4062: "exactly two literal arguments"), four take collection or
target names, one is DD#65's operand, one was closed by an earlier session,
and three are recursion from inside arms that are themselves cold.

That left exactly one candidate that could receive something else, and it is
the interesting one. `cmdsub_body_value`'s `_ =>` arm. Its only caller is
reached only from `expr_to_value`'s fall-through, and that match names every
`ExprKind` except `CommandSubstitution` and `Test` — so `Test` is the one
thing that could arrive. And `ExprKind::Test` is statement-only: built by
`parse_test_command` and `parse_attest`, both of which yield statements.
`validate` already depends on that exact guarantee and ICEs on it, with a
`#[should_panic]` test holding it.

So: unreachable, same guarantee, and a precedent in the same codebase for how
to say so. What made it worth doing rather than leaving cold is *what it did
instead of ICEing* — it returned `SsaValue::Expr("[[ ... ]]")`, carrying a
test's source text into a value position as an opaque placeholder. That is the
same shape as the bug where a `join` separator became the five characters
`"$sep"`. Silent-and-maybe-wrong, which is the option the repo rejects.

## What the three have in common

Nothing, in treatment: one became an ICE because a ruling made it unreachable,
one became three specs because an unrelated commit made it reachable, one
became an ICE because it had been answering a question it should refuse. The
ranking cannot tell them apart, and neither can coverage.

What they share is the method. All three turned on enumerating the *producer*
— the sites that construct the value — rather than the consumer or the
transform one assumes removes the shape. Journal 439 is where that rule came
from, and it paid for itself three times here: the `CmdSubst` producers, the
`ExprKind::Test` producers, and the `ExprKind::Literal` head guarantee are each
a producer-side argument, and each one is what settled the case.

## Left for DG

The `expr_to_string` task now has all three gates met, and the measurement has
an explanation: every caller is asking the same question — "what is the literal
name here?" — and the validator has already guaranteed the answer exists. The
general source-text renderer is an API that no caller wants.

Narrowing it to `literal_name_of()`, or making its arms ICE, is a change across
23 call sites and is hard to reverse. That is DG's call. The enumeration is
written down in the task file so he can make it without redoing the work.

## Postscript: a fourth, and the first one shadowed at *parse*

Re-ranked after refreshing `coverage.db` (which flagged exactly the three files
I had edited — the staleness check works). The new head was the
`expr_to_string` family, now blocked on DG, and `resolve_command`, already
triaged. The first actionable item was a pair of pipeline-stage functions.

Half of it needed nothing: `build_function_pipeline_stage`'s recursion guard
already carries a thorough comment explaining it is a DD#116 specification-
fidelity backstop, kept deliberately as a real diagnostic because "no function
is inlined into itself" is a statement that site is uniquely able to make.

The other half, `lower_one_pipeline_stage`'s compound branch, had no comment at
all. AGENTS.md asserts it is SB4022-shadowed; I checked by execution rather
than trusting the note, across eight spellings — `{ }` first and last in the
pipe, `while`/`if`/`for` heads, `(( … ))`, a redirected group, `case`. All
rejected: SB4022 six times, SB4021 once, SB4029 once.

**The notable part is *where*.** Every one of those is a *parser* diagnostic,
not a validator one. This arm is shadowed a pass earlier than the "validate-
shadowed" category assumes, which is worth knowing because it changes what
could ever reopen it.

What made it worth converting rather than annotating is what it did instead of
refusing: a complete, plausible lowering — fork a compound child into
`compound_pipeline_blocks` as `__compound_<id>` — that had never once run.
Relax SB4022 and that route opens in silence, with no spec behind it. Exactly
the shape the repo rule names. The sibling machinery stays live and tested
(`build_function_pipeline_stage` takes the same path for a function-call
stage), so nothing was lost.

The contrast with its neighbour is the useful thing to have written down: two
unreachable blocks, fifteen lines apart, with opposite correct treatments. One
states an invariant only it can state, so it stays a diagnostic. One states
nothing and merely duplicates a lowering for a form the language does not have,
so it ICEs. "Unreachable" was never the deciding question.

Sweep 8155 with the ICE in place — corroboration, not proof, since "merely
unexercised" is silent by definition. The `#[should_panic]` test is the part
that makes the claim falsifiable.
