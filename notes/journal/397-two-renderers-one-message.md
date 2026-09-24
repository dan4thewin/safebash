# 397 — two renderers, one message

The coverage queue pointed at `type_check.rs::check_scalar_assignment`, 8 cold
lines: the whole `String ← Int` arm, *"cannot assign int value to string
variable"*. The arm next to it was hit 505 times.

I nearly threw the finding away. `spec/types.sh` already has `hint.4`, which
asserts exactly that message and exactly that code, and it was passing.
`git grep` found **one** emitter in the tree. One emitter, a passing spec that
asserts its output, and a counter reading zero — one of those three had to be
lying, and the obvious suspect was the counter.

It wasn't. All three were true at once.

## What the counter was actually saying

The full function, source joined against counts, is unambiguous once you read
the *closing* lines rather than the arm:

```
1394   1007  fn check_scalar_assignment(
1403   1007      if d.var_type == VarType::Int && rtype == VarType::String {
1406    505          if !is_numeric_literal(value) {
1416    502      } else if d.var_type == VarType::String && rtype == VarType::Int {
1417      0          errors.push(TypeError {
1425    502      }
1426   1007  }
```

505 + 502 = 1007. The `else if` condition was **evaluated 502 times and was
never once true**. Not "the sweep missed this file" — the function ran a
thousand times and the predicate simply never held. That is a very specific
claim, and it is checkable.

## Answering by execution

A temporary `eprintln!` in the arm, and the two programs side by side:

```
decl  int n = 5; str s = $n   →  PROBE-HIT  +  SB4030 cannot assign int value…
assn  s=hi; int v=1; s=$v     →             ​  SB4030 cannot assign int value…
```

`hint.4` is the second line. It produces the message **without entering the
arm that contains the only copy of that message's text.**

The resolution is `type_check.rs:1341`, in `check_assign_type_match`:

```rust
"cannot assign {} value to {} variable `{}`{}",
    type_name(rtype), type_name(ltype), …
```

A *parameterised* renderer. It spells the identical sentence at runtime out of
`type_name()`, so a literal grep for the finished string cannot see it. Two
independent renderers, byte-identical output, split by a distinction nothing
in the message surfaces:

- **assignment** to a declared var → `check_assign_type_match` → 1341
- **declaration** → `check_scalar_assignment` → 1415 / 1426

`hint.4` exercises the first. The second had **no spec at all**.

## Why this one was invisible

The usual guard against an untested arm is "grep the message, find a spec that
asserts it". That guard fails by construction here: the spec asserts the text,
the text has one literal home, and the spec never goes there. Coverage was the
only instrument in the box that could tell the difference, because it counts
*lines*, not *sentences* — and it was right while every cheaper check agreed
with each other and were wrong.

This is the checkpoint-35 rival-renderer shape again (the arith briefs,
`599f3a2`), with a new wrinkle: there, the two renderers eventually diverged
and the divergence was the bug. Here they agree exactly, so there is no bug to
find today — only a spec hole that the duplication had made unreachable by
inspection.

## What landed

`decl.24`–`decl.28` in `types.sh`, the declaration side of the mismatch:

- `decl.24` `int n = 5; str s = $n` — and the hint text
- `decl.25` `str s = $(( 1 + 2 ))`
- `decl.26` `str s = $(f)` for `f -> int`
- `decl.27` control — `str s = 5` still declares (bare numeral is not an int)
- `decl.28` control — `str s = "$n"`, the form the hint recommends, runs

Non-vacuity by mutation rather than by revert, since there is no source change
to revert: `false &&` in front of the arm's condition fails **exactly**
`decl.24`/`.25`/`.26` and leaves both controls green.

Sweep 6171 → **6181**, as predicted (+5 × 2 modes). 0 failing, 0 flips.

## Left standing for DG

The two renderers remain. `VarType` is `{String, Int}`, so
`check_scalar_assignment`'s two arms are exhaustive over the same ground
`check_assign_type_match` already covers generically — the specialised copy
earns nothing that I can see, and it is one edit away from disagreeing with
the generic one in wording or in the `is_numeric_literal` exemption. Whether
to collapse it is a call I have not made.
