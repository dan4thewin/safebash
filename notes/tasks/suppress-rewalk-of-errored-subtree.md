---
title: "Should a pass skip re-walking a subtree it has already reported an error in?"
status: open
priority: P3
kind: design
depends: []
gates:
  - "DG decides whether this is a policy or a non-problem"
---

## Where it came from

DG, on the duplicate-diagnostic filing
(`duplicate-type-error-in-cmdsub.md`):

> "once you find an error in part of the tree, why walk that portion again?
> seems like that is a recipe for numerous errors"

That instinct found the bug — the filed diagnosis was wrong, and reframing it
as "why is this walked twice at all" exposed a forked walker, a live `len`
hole, and an ICE. All three are fixed.

The wider question it raises is **not** fixed, and is a genuine fork.

## The question

With the overlap removed, zero duplicates remain in a 66-program corpus. So
the structural fix was sufficient *for this instance*. The open question is
whether to also add a guard that makes any **future** overlap harmless:

> before walking a subtree, if a diagnostic has already been recorded inside
> that subtree's span, skip it.

## Why it is not obviously right

**For.** Overlaps are easy to reintroduce — `check_stmt`'s `cmdsub_bodies()`
walk is unconditional *by design* (its comment explains why), so any statement
kind that grows its own word-walker reintroduces the duplicate. That has now
happened at least twice (`check_command`'s args, `check_for`'s words). A guard
converts a whole class of future defect into silence rather than noise.

**Against.** It hides the structural problem instead of removing it. The
duplicate diagnostic is the only *symptom* of redundant work; suppress it and
the second walk still runs, still costs, and still mutates `env` — and the
next overlap goes unnoticed because nothing surfaces it. The three bugs in the
linked filing were all found *because* the duplication was visible.

**Also against.** "Already errored inside this span" is coarse. A subtree can
legitimately earn two unrelated diagnostics, and a span-keyed guard cannot
tell those from a double-walk.

## A third option

Make the redundancy itself an invariant rather than a judgement: have the
cmdsub walk record which bodies it has visited, and **assert** (debug-only) if
a body is visited twice. That surfaces an overlap at the moment it is
introduced, in the test suite, rather than suppressing its symptom — and it
costs nothing in release. The totality gate
(`contrib/totality-check.py`) is the precedent for this style: encode the
invariant where it can fail loudly.

## Recommendation

The third option, if any. The first two trade a visible defect for an
invisible one, and this codebase's recurring failure mode — journals 441, 456,
460, and the filing above — is precisely *invariants that no single function
owns and nothing checks*.

No action without DG.
