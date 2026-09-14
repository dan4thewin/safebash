---
title: "Audit: every diagnostic that recommends a spelling must recommend one that compiles"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "every diagnostic message in sash/src containing a suggested sash spelling is extracted and compiled"
  - "each suggestion either compiles clean or the message is corrected"
  - "a spec pins the corrected advice for each one found wrong — message text AND a run of the recommended form"
---

## Context

Two instances found on consecutive days, both by accident while doing
unrelated work. This is a class, not a coincidence.

- **SB4035** said "use the braced value form `${a[i]}`". `(strlen ${a[i]})`
  was **SB4020**. Fixed in `a231fb9` by widening `is_func_arg_start`.
- **SB5030** said "use `echo $(join @a)`". That is **SB5062** — `join` takes
  the array by name. Fixed alongside `converter-dollar-at-migration`.

The second one is the more alarming of the two. DD#100 line 119 **already
recorded it**, and deferred the cleanup into
`dd75-len-collection-only-strlen`, which is marked `done` with that gate
unmet. The bug survived being found, written down, and assigned.

## Why it recurs

A diagnostic's message is a string literal. Nothing compiles it, nothing
tests it, and it is written at the moment the *rejection* is implemented —
which is exactly when the author is thinking about what is invalid, not
about whether the alternative they are suggesting is valid. When the
accepted surface later moves (DD#75 §4 retiring `${#…}`, DD#106, the
`len`/`strlen` split), the rejection stays correct and the advice silently
rots.

The user-visible failure mode is the worst kind: the compiler tells you to
write something, you write it, and the compiler rejects that too.

## Approach

1. Extract every message containing a suggested spelling. The candidates
   are recognisable — they contain `` ` `` -quoted sash fragments, usually
   after "use", "write", or "try". `string_literals` in the indexnudge DB
   with `context = 'arg'` and `value LIKE '%use `%'` gets most of them;
   `validate.rs`, `type_check.rs`, and `parse/error.rs` are the files.
2. For each, build the smallest program that *reaches* the diagnostic, then
   the same program with the suggestion substituted, and compile both.
   Assert the first fails with that code and the second exits 0.
3. Correct the messages that fail.
4. Spec each correction the way `gs.36`/`gs.37` do it: one test asserting
   the message text, one **compiling and running** the recommended form.
   Asserting message text alone is what let SB5030 persist.

Step 2 is the part worth automating — a harness that does it generically
would keep the class closed rather than fixing the current instances. Worth
considering whether it belongs in the spec suite as a standing gate.

## Also worth checking while in there

`dd75-len-collection-only-strlen` is marked `done` with DD#100's
message-cleanup gate unmet. Re-open it or fold the remainder here, and
check whether anything else closed out the same way.

Stale comments at `validate.rs:511` describe `len @a` / `join @a` as "open
questions". They are settled — SB5062. Non-user-facing, so left alone, but
they are the same rot in comment form.
