---
title: "Should a `$( )` body that taints an outer global be SB5067?"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG has chosen between extending SB5067 to bodies and merging taint outward"
---

# A `$( )` body can taint an outer global and nothing says so

Opened 2026-09-29 by the fix in journal 445, which closed the surrounding
escape but deliberately left this half unmodelled.

## Measured

A **function** body tainting a global is a located `[SB5067]`.

A **`$( )`** body doing the same reaches only the SSA backstop — no SB code,
no span. And the write really does take effect, per the DD#86 addendum:

```sash
str t=clean
str q=$(t=modified; /bin/echo x)
```

prints `modified`. So the value escapes and the taint does not follow it.

## Why it is not simply a bug to fix

`taint_cmdsub_bodies` clones the tainted set per body, copying
`taint_function_body`. Clone-and-discard is sound *for functions* precisely
because SB5067 makes the top level the only place a global's taint can arrive.
`taint_top_level` documents the consequence: "the set this leaves behind is
final — no fixpoint". That invariant does not hold for `$( )` bodies, so the
choice here is an architecture choice, not a patch.

This is strictly better than the previous state (the pass walked nothing at
all) and introduces no new hole, which is why it shipped.

## The options

**(a) Extend SB5067 to `$( )` bodies.** Preserves the no-fixpoint
architecture, and is what "a body is an anonymous function" implies — the same
rule, applied to the same construct. Cost: it would newly reject programs that
taint inside a body and attest after it, which compile today.

**(b) Merge taint back out for names the enclosing scope binds.** Models DD#86
exactly. Cost: needs a bound-name set threaded into the descent, and gives up
the no-fixpoint invariant that `taint_top_level` currently relies on.

Recommendation: (a), because it keeps one rule for one construct and the
architecture it protects is load-bearing. But it is a behaviour change that
rejects currently-valid programs, so it is DG's call.

## Update 2026-10-01 — (a) got much cheaper

DD#121's addendum relaxed SB5067 from "may not taint a global" to "may not
*leave* a global tainted" (no net insert at body exit). That changes this
question's arithmetic.

The stated cost of (a) was that it "would newly reject programs that taint
inside a body and attest after it". Under the net-effect rule an **in-body
attest is the repair** — a `$( )` body could taint an outer global and attest
it adjacently, exactly as a function body now can, and be accepted. What (a)
would still reject is the genuinely leaky shape: a body that taints an outer
global and leaves it that way.

The rationale for (a) also got stronger. The task's own argument is that
clone-and-discard is sound for functions "precisely because SB5067 makes the
top level the only place a global's taint can arrive" — and the net-effect
rule is what now carries that invariant, in the form
`entry ⊇ exit` for globals. Extending the *same* predicate to `$( )` bodies
is a smaller step than extending the old syntactic one was.

Status quo is preserved for now and deliberately: the addendum's rewrite moved
site collection to `walk_scoped_structural_stmts`, which does not descend into
`$( )` bodies, so this question is exactly where it was. Still DG's call.
