---
status: blocked
priority: P3
depends:
gates: DG decides whether `(...)` may carry a counted repetition
note: the rejection is shipped, specced (39.28/39.29) and documented in
  docs/sash.1 as an engine limitation (DG, 2026-09-14). Only "should it be
  *supported*" is still open, and nothing is blocked on it.
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

## Measured cost of supporting it (2026-09-14)

DG asked whether the span replay "can be accomplished as a tiny change".
Four pieces, and only the first two are tiny.

**1. Track a span, not a point (~8 lines).** Add `prev_atom_end` next to
`prev_atom_start`, and a small stack of `(` source positions parallel to
`nbra`, so that on RBRA `prev_atom_start` can be set to the matching `(`
rather than to the group's last atom.

**2. Replay the span (~12 lines + 5 call sites).** The five existing
`exprp = prev_atom_start; token = lex(…); operand(token);` sequences become
one helper that loops until `exprp >= end` and dispatches
`operator()`/`operand()` exactly as the main loop does — the replayed span
contains LBRA/RBRA, which are operators, so a plain `operand()` will not do.

**3. Capture-group identity (~5 lines, but this is where the risk is).**
`operator(LBRA)` does `++cursubid`, so each replayed copy would allocate a
**new** capture group: `(ab){2}` would report `nsub = 2` where POSIX says
one group holding the last iteration. `nsub` is observable — DD#63 surfaces
it as the group count, and `sash-regex.c` reads it to size the match vector
— so this is not cosmetic. The fix is to reset `cursubid` to its pre-span
value at the start of each replayed copy, which makes every copy re-derive
the same ids and leaves `cursubid` at the right maximum afterwards. Small,
but it mutates shared state inside `operator()`, which every other path
also uses.

**4. Re-derive the slot budget (~1 line, needs an argument).** The current
`nslots += 128` per `{` is justified in a comment: 32 copies × (one operand
+ an operator emitting at most two) = 96, with margin. A *group* copy costs
`2 + ~3·atoms` instead of 3, so a 32× repetition of a ten-atom group needs
roughly a thousand slots and blows the budget. It must be budgeted from the
span length rather than a constant. `newinst` enforces the cap regardless,
so an under-budget is a clean compile error rather than corruption — which
is the one comfort here.

**Verdict: ~25 lines of mechanism, plus one shared-state change that needs
care and one budget derivation that needs defending.** Not a one-liner, but
not large either. Specs 39.28/39.29 pin the rejection and would flip.

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
