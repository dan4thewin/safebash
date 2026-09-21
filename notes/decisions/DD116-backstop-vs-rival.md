---
title: "Backstop vs. rival: when unreachable code stays and when it goes"
status: accepted
date: 2026-09-21
---

# DD#116 — Backstop vs. rival

## The question

Cold-line work keeps turning up code that cannot be reached. Some of it is
worth keeping as a regression shield; some of it is dead weight that should
go. Before this DD the call was made case by case, and the reasoning was
re-derived each time.

"Is it unreachable?" does not decide it. Four unreachable things in this tree
were deliberately **kept**, and one was deliberately **deleted**, all for good
reasons. The distinction is not reachability.

## The test

> **Does this code BACKSTOP a decision, or RIVAL it?**

A **backstop** sits *downstream* of an authoritative decision and catches the
case where that decision is wrong or changes. It has no opinion of its own.

A **rival** is a *second independent answer* to a question that is already
answered authoritatively somewhere else. It looks defensive, but it is a
duplicate — and duplicates drift, silently.

**A rival is dead code however defensive it looks. Delete it.**

A backstop earns its place if it has **at least one** of:

1. **Loud-vs-silent.** The fallback it shadows degrades *silently wrong*, not
   merely less informative. Deleting it turns a loud failure into a plausible
   wrong answer.
2. **Totality.** Its presence buys a compile-time guarantee — typically an
   exhaustive `match`, so a new variant is a compile error *at this site*
   before it is a bug anywhere else.
3. **Specification fidelity.** It states the rule as *specified*, not the
   shape the current pass ordering happens to deliver, so correctness survives
   a reordering.

If it has none of the three, it is not a backstop either — it is just
unreachable. Delete it.

## Why rivalry overrides

A rival that is *currently* in agreement is invisible. Nothing fails, the
tests stay green, and the copies drift apart one edit at a time. The
`logos_tokens.rs` keyword list is the canonical case: 27 variants that could
never match, retained "as guard rails", which amounted to "a second, silent
answer to *what is a keyword?* sitting beside `classify_word_token`'s real one
and free to drift out of step with it."

The cost of a rival is not the lines. It is that reading the output cannot
tell you which copy produced it. That is precisely what let a dead arm in
`arith_brief` survive: the message it would have rendered was
character-identical to the one three other sites actually produce.

## Precedents (this DD codifies existing practice)

| site | verdict | property |
|---|---|---|
| `transform/expr_handlers.rs` — `len`/`join` arrvals arms | keep | 1 — the fallback emits `strlen` of an undefined var, a silent `0` where a count belongs |
| `transform/expressions.rs` — `expr_to_string`'s six unreachable arms | keep | 2 — exhaustive match makes a new `ExprKind` a compile error here first |
| `transform/capture_purity.rs` — the function-body walk | keep | 3 — makes the disqualification correct for any call seen before inlining flattens it |
| `validate.rs` — the DD#31 non-final-comma mutation test | keep | 3 — states what DD#31 specifies, not what the current ordering delivers |
| `parse/logos_tokens.rs` — 27 keyword variants | **delete** | rival to `classify_word_token` |
| `validate.rs` — `arith_brief`'s `Assign`/`CompoundAssign` arms | **delete** | rival — a third copy of the `x = ...` brief |
| `parse/arith.rs` — `tok_to_text`'s paren arms | keep | 2 — subsumed: the whole renderer was made total, see below |

## When the rival test is the wrong question

`tok_to_text` entered this DD as a clean rival: its four paren arms are
unreachable because `step_arith_collect` renders `(`, `((` and `))` itself
before the default token path runs. That reading was correct and still
beside the point.

Proving it by execution (as required below) showed the `_ => String::new()`
fallback was *reachable* — and that it silently deleted **35 of the 59**
token kinds from the collected arithmetic text. The re-lexer never saw them,
so trailing garbage disappeared:

```
(( x = 1 done ))     sash: compiles, x=1      bash: syntax error
```

21 of 22 keywords behaved this way, in both `(( ))` and `$(( ))`.

The lesson is about *diagnosis order*. A rival is a redundant answer to a
question already answered correctly. Here the function was not answering
correctly at all: it is a total map from token to text, and it was wrong for
most of its domain. Deleting the four arms would have been tidy and would
have left the bug untouched.

**Before asking whether a branch is a rival, check that the function it sits
in is right.** Redundancy is only worth arguing about above a correct
baseline.

The fix was property 2 applied to the whole function: every variant now has
a deliberate rendering and there is no `_`, so a new `Tok` is a compile error
here rather than another silent drop. The paren arms survive as ordinary
members of a total match — which is why their row above now reads *keep*.


- **Keeping requires a written reason naming the property.** A comment that
  says only "kept as a guard" is not enough; it must say *which* of the three
  and why. The kept precedents above already do this, and that is the standard.
- **A shield belongs at the fallback, not duplicated per-arm.** If the concern
  is that a `_` arm degrades silently, fix the `_` arm once. That protects
  every case rather than the handful someone thought of, and it does not
  create rivals. **How** to make it loud depends on where the input comes
  from: at an internal boundary a `panic!`/`unreachable!` is right, but where
  the variant is reachable from user source — as `tok_to_text`'s was, via
  `(( if ))` — a panic converts a diagnosable error into an ICE. There, loud
  means *compile-time*: drop the `_` and make the match total.
- **Prove it before acting.** Cold coverage is a *hint*, not a finding — a
  cold line may equally be a bug, an untested path, or dead code, and the
  report cannot tell them apart. Establish unreachability by execution:
  replace the body with a `panic!`, run the full sweep plus probes aimed at
  the shape, and require zero hits.
- **Deleting a rival is not a behaviour change**, which is why it needs no
  spec. If deleting it *does* change behaviour, it was not unreachable and the
  premise was wrong.

## Scope

Applies to `sash/src`. It is a rule for *unreachable* code only. Reachable
redundancy is an ordinary refactoring question and is out of scope here.
