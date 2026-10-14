---
title: The rule that nothing asserted
date: 2026-10-13
---

# The rule that nothing asserted

DG asked about "keeping tasks out of the queue" — the two backlog tasks that
each end with a note I wrote on 2026-09-12 saying *re-estimate before
scheduling*. Neither gate was ever discharged, so neither task could move, and
they had been sitting on their own instructions for a month. He wanted them
done, not described again.

Both re-estimates turned out to be wrong in the same direction: the filing was
a snapshot of a moving tree, and so was the re-measurement that replaced it.

## r47 — the goal was met while the task sat blocked

The task asks for `sash/tests/validate/sb{NNNN}.rs`, one file per rule.
`sash/tests/` has not existed since journal 304 deleted it — and deleted it
because the harness was **fail-open**, calling `compile_to_object` directly so
`validate` never ran. A suite of per-rule validator tests is the one thing that
harness structurally could not have provided. The sizing was wrong for the
third time running (1087 filed, 7222 re-measured, **11745** actual; ~14 codes
enumerated, **93** actual).

But the *goal* is testable independently of the artefact, and one `comm` answers
it: **92 of 93** SB codes raised by `validate.rs` are named by some spec.

The exception was **SB5027** — the for-in sole-source rule — and it failed in
exactly the shape the task was filed to catch. The line is *warm*: six hits in
`coverage.db`. Three specs, `av.m1`-`av.m3`, run programs that raise it. All
three asserted only `-n $err`.

Those programs also trip SB5020, because an unattested `$(argv)` splat is
tainted. SB5020 fires first and satisfies `-n $err` by itself. So the rule
could have been deleted outright and all three specs would have stayed green.

**A warm line is not coverage, and "some program reaches the rule" is not an
assertion.** That is the finding; the one-line `comm` is worth more than the
suite the task asked for.

## The fix is at the site that already owns the behaviour

My first move was to write two new specs at `60.73`/`60.74`. The control —
`for x in $(argv)` compiles and iterates — turned out to be a verbatim rival of
the existing `av.s1`, which asserts the same thing on the same program. Writing
it would have failed DG's rival test on its first day. And the negative
duplicated shapes `av.m1`-`av.m3` already run.

So: no new ids. Tighten the three existing specs to name **both** codes, and
say in the section comment why the weaker assertion was insufficient. The
suite's plan is unchanged at 81 and the sweep total is unchanged at 8367 — the
hole was never a missing program, only a missing claim.

**Proved by reversion.** Stubbing `check_for_header_splat`'s condition to
`false` and rebuilding turns **exactly three** specs red. Before the tightening
the same stub turned **none**. That number is the whole result: a gate that
cannot fail is not a gate, and this one could not.

## audit-string-keyed — 57, and the divergence is correct

The 2026-09-12 re-measurement said 153 and "the surface has grown several-fold".
It had not: 21 of those sites were in `emit_janet.rs` / `emit_janet_map.rs`,
**deleted by DD#104**, and the query counted argument-position strings rather
than dispatch. The real figure is **57**.

Most of it is category 2 — "is this *this specific* builtin?" — which
`from_name` cannot improve, one name on each side. The part worth having is
category 3, and inspecting validate.rs's 25 sites found it: **six** functions
answering "which argument is the out-target?" from six hand-written name lists,
and the lists **disagree**. `mapfile` in two of them, absent from two others.
`readline` in one, absent from another.

That is the shape that has produced six bugs in this codebase already, so I
treated it as a possible bug under the zero-tolerance policy and checked before
anything else. It is not one:

- `readline` is absent from `insert_auto_collection_target` **by design** —
  DD#99: "`readline` binds a scalar", so it never auto-creates a collection.
- `mapfile` is **vestigial**. DD#99 stage 6 retired it; SB5060 rejects the name
  before any of these predicates matter.

The honest scope is therefore not a 57-site sweep. It is: branch-probe and
delete three dead `mapfile` alternatives, and decide whether six copies of one
question should be one function. The latter is journal 506's lesson again — six
copies is six chances for a fix to land on five. Re-filed at P4.

Worth recording that the `mapfile` conclusion is a **hypothesis**: line
coverage cannot see which alternative of a `matches!` fired, and nobody has
probed the branch. Stated as such in the task rather than as a result.

## Gates

Sweep **8367** (4068 c + 13 skipped + 4286 native), zero failures, zero flips,
zero new ids — all three predicted before running. Liveness 258. `mk test` 124
+ 27 + 20.
