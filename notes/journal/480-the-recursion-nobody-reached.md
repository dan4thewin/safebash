# 480 — the recursion nobody reached

Two more cold lines off the coverage queue. Both were category 1 — genuine
spec gaps — and both were cold for the same structural reason: the suite
exercised the *shape* in question many times over, but never in the one
configuration that makes the walker recurse.

## `walk_arith_subscripts`'s `SubscriptAssign` arm

`ir.rs:1068-1074`. The arm visits the subject name, then recurses into the
index expression and into the assigned value. Three of its lines were cold.

The surprise was that `(( a[0] = 99 ))` is already specced —
`spec/arithmetic.sh:575` has had it for a long time. So the arm looked like it
should be warm, and the first guess was that the ranking had gone stale.

It had not. The arm's *first* line (the subject visit) is warm; what was cold
was the recursion. Every subscript-assignment in the suite assigns a plain
literal to an int array:

    (( a[0] = 99 ))

— nothing nested, so neither `index` nor `value` contains a second cell for the
walk to find. And the obvious way to put a *bad* cell in assign position,

    (( m[0] = 7 ))

does not reach this walker at all: DD#105's int-key rule rejects a map subject
first (that is `bsx.7`). So the forms that would warm the recursion all needed a
*good* subject with a *bad* cell nested inside it, and nothing in 7809 checks
had that shape.

The three that do:

| program | diagnostic |
|---|---|
| `int map m=([k]=5); int array b=(1 2); (( b[0] = m[0] ))` | SB4030 bare map cell |
| `array s=(x y);     int array b=(1 2); (( b[0] = s[0] ))` | SB4030 string cell (DD#77 D1) |
| `int map m=([k]=5); int array b=(1 2); (( b[m[0]] = 9 ))` | SB4030 bare map cell, via the *index* |

The third is the one worth having separately: the first two could both be
satisfied by a walker that recursed into `value` and forgot `index`.

These are `bsx.11`-`bsx.13` in `spec/arithmetic.sh`.

**Why it matters.** A missing recursion here is silent in the worst way. The
walker feeds `type_check::check_arith_bare_map_subscript`, so a dropped branch
is not a crash and not a wrong message — it is *no* message. The offending
read compiles, and a map cell gets read under arithmetic rules that cannot
express its key.

## `take_end_block`'s error arm

`parse/parser.rs:49-51` — the `Err` branch of `parse_end_block`: push the
diagnostic, recover, return.

An earlier probe in this queue had tried `END { $$$ }` and got a *compile*,
because `$$$` is not a syntax error. The shape that reaches the arm is simply
an END body that never closes:

    echo hi
    END {
    /bin/echo bye

which reports `SB4014 expected '}'`. Every `END { }` in the suite parses
cleanly, so the one path that reports a malformed one had never run.

This arm is worth a spec for what it does *after* the message as much as for
the message: it recovers and keeps parsing rather than registering a
half-finished END body. A regression that reported the error but kept the
partial body would hand a later pass an END block the parser never completed.

That is `56.88` in `spec/signals.sh`.

## The pattern, stated once

Both gaps share a shape that the coverage ranking cannot distinguish from a
dead arm, and that a reading of the suite *also* cannot: the construct is
specced, repeatedly, but only in its simple form. The cold line is not the
construct, it is the construct **composed with itself**.

Grepping the suite for `(( a[` finds plenty and suggests the arm is covered.
Only the per-line data says otherwise, and only by asking what the *arm* does
— recurse — rather than what the *statement* is.

## Gates

- predicted 7817 before running (7809 + 2×4, both files dual-mode); sweep
  returned exactly 7817, **0 failing**
- 0 flips; exactly the 8 expected new ids; regression self-test 248
- `mk test` 85 + 27 + 20
- both collision checks clean; plan-vs-chk mismatch only `coverage.sh` (+2, the
  known variable-id loop)

No compiler source changed — this commit is specs only.
