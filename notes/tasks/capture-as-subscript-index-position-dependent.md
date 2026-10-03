---
title: "A capture as a subscript index: accepted in a declaration, SB4030 in an argument"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "DG rules on whether DD#80 permits `${a[$(f)]}`"
  - "whichever way, the three positions agree"
  - "the ICE below is closed"
---

## Two bugs, one undecided question between them

### 1. The ICE

```sh
array a=(x)
str v="${a[foo]}"
```

```
thread 'main' panicked at sash/src/codegen/values.rs:1426:
non-integer literal `foo` reached int lowering;
type_check (SB4030) should have rejected it
```

The compiler says out loud that a pass it relies on never ran. `echo
"${a[foo]}"` is a clean SB4030; the same expression as a declaration's value
is an ICE. Pre-existing — confirmed against a pristine build of `0d72da5` in a
scratch worktree.

**Cause.** A command walks its arguments with `check_subscript_types`.
`check_assignment` walks its *target's* index (`a[i]=…`) and
`check_declaration` walks nothing, so an expression on the right-hand side is
never offered to the subscript rules at all.

Also reachable as a bare assignment (`v="${a[foo]}"`).

### 2. The inconsistency

The one-line fix — walk the value — was written, and it regressed `lift.3`,
which turned out to be pinning the other half of the same disagreement:

| | HEAD |
|---|---|
| `str z=${arr[$(f)]}` (spec `lift.3`) | **accepted**, prints `q` |
| `echo "${arr[$(f)]}"` | SB4030 |
| `int n=${arr[$(f)]}` | SB4030 |

Same expression, three positions, two answers. Walking the value makes the
declaration agree with the other two — which **breaks a passing spec of a
deliberate feature**.

`functions.sh` lift.1-lift.3 are a block about capture *lifting*: a capture as
an array element, as a map value, and as a subscript index. lift.3's own
comment shows the author thought about it — *"(`q`, not `p` — a vacuous index
would collapse to 0 and still print the first element)"*.

## The question for DG

`is_int_index` is `is_integer_literal(expr) || expr_type(expr, env) ==
VarType::Int`. A capture types as String, so it fails. DD#80 says an indexed
array's index *"must yield a number"* — and a capture yields a number only at
**runtime**, which is the guarantee DD#80 exists to provide statically.

**(a) The capture index is legal; the argument position is the bug.** Teach
`is_int_index` to accept `CommandSubstitution`. Honours lift.1-3 as the
deliberate feature block they read as. But it admits a value whose
numeric-ness nothing checks, and lift.3's own comment concedes a non-numeric
capture silently collapses to 0 — the exact failure DD#80 names.

**(b) lift.3 pins a hole; the rejection is right.** Walk the value, let the
declaration join the other two positions, retire lift.3 and require
`${arr[$(( $(f) ))]}` or an `int` temporary. Consistent and static, but it
breaks working programs and contradicts a shipped spec.

**(c) Legal but must be converted** — accept a capture only inside `$(( … ))`,
which is already true, and make lift.3's bare form an error with that advice.
This is (b) with a better diagnostic.

No recommendation: this is a language-surface call, and the evidence points
both ways — the spec block says the feature is intended, DD#80's text says it
cannot be guaranteed.

## Note

The ICE cannot be closed without answering this, because the only sane fix for
it is the walk that exposes the disagreement.

Found while fixing `duplicate-type-error-in-cmdsub.md`; the forked walker
there was masking this one too — the deleted fork had an
`Assignment => check_subscript_types(&a.value, …)` arm that only ever ran
inside a `$( … )`.
