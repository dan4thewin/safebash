---
title: "bug: janet emits the out-array `var` inside the if-condition expression"
status: deferred
deferred-by: DD104
priority: P2
kind: bug
depends: []
gates:
  - "`if matchall abc \"[a-c]\" g { echo hit }` runs under --janet"
  - "`if match abc \"(a)(b)\" g { echo hit }` likewise"
  - "spec 63.mi1-63.mi3 run in janet mode instead of being skipped"
---

## What

The DD#63 out-array form auto-declares its target. In the janet backend
`emit_match_into_condition` emits that declaration with `predeclare_one`,
but it is called while the emitter is mid-expression — building the `if`
condition — so the `var` lands *in condition position*:

```janet
(if (var g @[])
(let [_f (os/spawn ["sash-regex" "captures-all" "abc" "[a-c]"] …)] … )
  (do …))
```

`(var g @[])` becomes the condition, the real match expression becomes the
then-branch, and the body becomes the else-branch. janet then rejects the
program outright:

```
j.janet:13:29: compile error: unknown symbol g
```

## Pre-existing

Reproduces with `matchall` at `c1e9807`, before `match` was given the same
auto-declare (`bug-match-outarray-no-autodeclare`, fixed 2026-09-11). That
fix makes singular `match` reach the same broken path, which is why spec
63.mi1-63.mi3 are `in_modes native,c` with `skip_chk` in janet — remove the
gate when this is fixed.

Only the *condition* position is affected. Statement position
(`emit_janet_match`) predeclares into the statement stream and is fine, which
is why 63.mi4 (pre-declared array, no predeclare emitted) passes in janet.

## Shape of the fix

The `var` has to be hoisted out of the expression being built — emitted to
the enclosing statement stream ahead of the `(if …)`, the way
`emit_janet_match` already does in statement position. That needs a
pending-preamble channel on `EmitCtx` (or the caller emitting the
declaration before it starts writing the condition), not a write to the
current `out`.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
