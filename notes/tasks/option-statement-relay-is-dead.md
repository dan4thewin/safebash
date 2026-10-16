---
title: The Option<Statement> relay is dead — 8 cold lines and a third false diagnostic
status: done
priority: P2
kind: hardening
source: journal 509 follow-up (2026-10-15)
depends: []
gates:
  - full sweep green, zero flips
  - mk test green
  - mk cov-union shows parser.rs cold cohort down from 20
---

## What was measured

Answering DG's "does C reduce cold line count?" for journal 509's three-way
fork. Measured against `coverage.db` at `f91e1ae` (98.23% line, union of spec
and unit tests): **8 of `parse/parser.rs`'s 20 cold lines are the one
`Result<Option<Statement>, Diagnostic>` protocol.**

| line (@f91e1ae) | text | function |
|---|---|---|
| 237 | `Ok(None) => {}` | `parse_top_level_item` |
| 279 | `None => return Ok(None),` | `parse_statement` |
| 425 | `None => break,` | `parse_body_until` |
| 705 | SB4027 — now `unreachable!()` (7079193) | `parse_brace_else` |
| 1240 | `None => Err(return_producer_error(span)),` | `parse_return` |
| 1260 | `return Ok(None);` | `parse_negated` |
| 1341 | `Ok(None)` | `parse_suppressed` |
| 3169 | `None => return Ok(None),` | `parse_list_operand` |

Only one `Ok(None)` origin is warm: `parse_statement:270`, the EOF /
comment-only check. Both `parse_top_level_item` and `parse_body_until` call
`scanner.at_eof()` *immediately before* calling `parse_statement`, which is why
their `None` arms are cold even though the origin is not — the three remaining
callers (`expr.rs:1703`, `parse_suppressed:1350`, `parse_trailing_pipeline:2908`)
are what keep line 270 alive.

33 production call sites of the cohort were enumerated from the index: **31 are
tail propagations** (`return parse_x(..)` or a tail expression), so the `Option`
flows through untouched and narrowing deletes no handling code there. Exactly
**two** destructure it — `parse_brace_else` (now the ICE) and `parse_return`.
No callers outside `sash/src/parse/`.

## The finding worth chasing on its own

`parse_return:1240` is a **third unreachable user-facing diagnostic**, the same
family as the two closed in 7079193: `return_producer_error`, i.e. SB4014
"return: expected a status producer".

It also sharpens AGENTS.md's "a diagnostic CODE is not a site" rule, because
here the **message** is not a site either. `return_producer_error` has two call
sites — `parse_return_value_form:1221` and `parse_return:1252` — raising
*byte-identical* text. Probing `fn f() { return - }` produces that message and
looks like proof the arm is live; it fires 1221. 1252 stays cold. A spec
asserting the message text would have been accepted as cover for the wrong one.

**Before acting, probe 1252 directly** — a cold line is a hypothesis. The only
route is `parse_return_producer_head` returning `None`, whose sole source is its
`Tok::Bang => parse_negated(scanner)` arm, and `parse_negated`'s own `Ok(None)`
(1260) is itself cold.

## Resolution (2026-10-15, journal 510) — DG chose C

Done, and it closed **all eight** lines rather than the one C was costed for.
Twenty-five signatures narrowed after examining their producers and caller
guards; the two remaining `Option`s retain live EOF cases. Sweep 8377, zero
flips, zero new ids. Journal 511 corrects the original resolution's
"self-proving" claim: compiling after changing branches and signatures checks
the new contract, not whether the removed branches were unreachable before.

## Why this was blocked on DG (historical)

It is the back half of journal 509's fork, which is DG's call:

- **A** (committed) — leave the signatures; one `unreachable!()`. Note that
  `parse_brace_else`'s ICE **cannot be unit-tested** (reaching it requires
  editing `parse_if`), so under A that line is cold *permanently* — outside the
  spec ∪ unit-test union that defines the 100% target.
- **B** — narrow `parse_if` alone. Removes line 705 at the type level; makes 1
  of 27 signatures odd and adds `.map(Some)` at `dispatch_statement:298`.
- **C** — narrow all 22 never-`None` functions. Removes 705; the other seven
  belong to the four functions that *can* relay a `None`, so closing them is a
  further step (collapse the relay once 1240/1260/1341 are probed).

The measurement changes C's cost/benefit: it is not churn for one line, it is
the entry point to 40% of `parser.rs`'s cold cohort plus a false diagnostic.
