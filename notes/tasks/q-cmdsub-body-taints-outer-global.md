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
