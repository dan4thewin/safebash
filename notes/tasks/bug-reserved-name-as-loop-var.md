title: "A `for` variable could take a reserved collection name"
status: done

# A `for` variable could take a reserved collection name

Found 2026-09-17 by probing the four cold branches in
`type_check.rs::check_len_operand`. Line 1058 (`env.is_slot(&name)`) bails out
silently; asking what could reach it produced this.

## Symptom

```sh
fn f {
  for A in @B { echo "$A -> $(len A)" }
}
array a=(p q r)
array b=(xx yyy)
f =a =b
```

Printed `xx -> 3` / `yyy -> 3`. One name, two simultaneous live meanings:
`$A` read the **loop variable** (`xx`, `yyy`) while `len A` read **slot A**
(array `a`, length 3). `join A ,` gave `p,q,r` and `${A[0]}` gave `p`.

## Cause

DD#100 Part B reserves `A`–`E` "on the `args` precedent: rejected as user
variables at top level and inside functions". Nothing enforced that for a
`for` variable, because a for variable falls between the two gates that
would otherwise catch it:

- **DD#86 / SB5049** (no shadowing) explicitly exempts it — *"A `for` variable
  is not a declaration and is untouched."*
- **SB5052** (`args` immutability) gates *writes* — assignments, declarations,
  and bulk fills — and a loop variable is none of those `StmtKind`s.

`ir.rs:361`'s claim that slot names "can't be declared as user variables" was
true of *declarations* specifically; a for variable introduces a name without
being one.

At top level the collision is already caught by SB5065 (slot used outside a
function), so only the in-function case was reachable.

## Fix

**SB5080** — a `for` variable may not be named with a collection-slot letter
(`A`–`E`) or `args`. Lowercase names that merely resemble a slot (`for e in
…`) stay legal; the slot namespace is the single uppercase letters only.

## Scope — what this deliberately does NOT do

DG initially chose the general form ("extend no-shadowing to loop variables vs
any existing declared name"), pending an audit of every construct that
introduces a name without a declaration. The audit changed the answer, so the
narrow rule shipped instead:

| pattern                                   | verdict |
|-------------------------------------------|---------|
| loop var = slot letter `A`–`E`            | **bug** — two live meanings |
| loop var = `args`                         | **bug** — reserved, immutable |
| loop var reuses a top-level global's name | fine — one variable, overwritten; bash-identical |
| loop var reuses a function-local's name   | fine — same |
| inner loop var reuses outer loop var name | fine — same |
| two sequential loops reuse a name         | fine — same |
| loop var = slot letter, at top level      | already SB5065 |

The last five are **one name with one meaning**, which is what DD#86 asks for;
rejecting them would contradict the DD's explicit for-variable exemption and
reject bash-compatible programs. Only the reserved-namespace collision creates
a second simultaneous meaning, and that is what SB5080 rejects.

## Why it survived

The reservation was stated in DD#100 prose but enforced only for the
`StmtKind`s that existed when it was written.
