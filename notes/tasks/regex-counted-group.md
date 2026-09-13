---
status: blocked
priority: P3
depends:
gates: DG decides whether `(...)` may carry a counted repetition
---

# Counted repetition of a group: reject (now) or support?

`(ab){2}` and `(a|b){4}` used to compile and return the **wrong answer**
silently — sash said `N` where bash says `Y`. As of journal 290 they are
**rejected** with "counted repetition of a ( ) group is not supported".
Rejecting was the conservative, reversible half of the fix; whether to
support them is DG's call.

## Why it was wrong

In `regcomp.c`'s main compile loop, `prev_atom_start = atom_start` is
assigned only when `(token & 0300) != OPERATOR`. Both `(` (LBRA) and `)`
(RBRA) are operators, so after lexing `(ab)` the variable still points at
`b` — the group's **last atom**, not its opening paren. The `COUNTED`
expansion branch then re-lexes exactly **one** token per copy, so `(ab){2}`
compiled as `(ab)b`.

Note what the shape of the bug guarantees: every affected pattern is one
that *looks* like it works. `(ab){2}` against `abab` happens to be the
case a reader would check, and `(ab)b` does not match it, so the symptom
is a clean `N` with no diagnostic anywhere.

## Options

1. **Keep the rejection.** `{n,m}` is a sash addition to Pike's engine
   (DD#39), and DD#39 states a deliberate pre-feature-creep posture. A
   diagnostic at compile time is honest about the limit and costs nothing
   at runtime. Users write `(ab)(ab)` — which works today.
2. **Support it** by replaying a token **span** rather than a token:
   record `prev_atom_end` alongside `prev_atom_start`, and have the
   expansion loop run `while (exprp < prev_atom_end) lex()` per copy. The
   slot budget already accommodates it (128 slots per `{`, plus
   `newinst`'s hard bounds check), so this is a compile-loop change, not
   an allocator change. Cost: the expansion is no longer bounded by "one
   atom × 32" but by "group length × 32", and nested groups with nested
   counts (`((ab){2}){2}`) multiply. The `newinst` guard turns any missed
   case into a diagnostic rather than corruption, so the failure mode is
   safe either way — but the budget arithmetic would need re-deriving.

## Recommendation

Option 1 until someone has a real pattern that needs it. The engine is a
compile-time validator *and* a runtime matcher shipped in `sash-rt`;
every construct added there is code that must stay correct under
adversarial input. `(ab)(ab)` is not a hardship.

## Specs

`spec/regex.sh` 39.28, 39.29 pin the rejection and reference this file by
name. 39.30, 39.31 pin the constructs that are *not* affected (`[ab]{4}`,
`a*{2}`) so a future implementation can't quietly change them.
