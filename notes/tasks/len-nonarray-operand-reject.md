title: "`len` on a non-array operand: garbage (string) / segfault (undefined) — reject at compile time"
status: done
superseded_by: dd75-len-polymorphic (DD#75 §2)
priority: P1
kind: bug
depends: []
gates:
  - "`len` on a scalar string is a COMPILE error (SB error, e.g. `len requires an array — use ${#s} for a string's length`), not runtime garbage"
  - "`len` on an undefined/never-declared name is a COMPILE error, not a runtime SEGFAULT"
  - "`len arr` on a real array still works (element count)"
  - "the misleading `BuiltinCmd::Len` doc comment (`\"string or array length\"`) is corrected to array-only (per DD#41)"
  - "0 native spec regressions; add specs pinning the two rejects + the working array case"

# `len` is array-only but silently misbehaves on non-array operands

## Discovery

Found 2026-08-04 while fixing `$(len a)` int-typing (DG's demo bug). Per
**DD#41**, `len`/`keys`/`join` are **array** operations. But `len` on a
non-array operand doesn't reject — it produces garbage or crashes:

```
s=hello; echo $(len s)        # → 3856007167   (garbage; strlen NOT computed)
echo $(len undefined_arr)     # → SEGFAULT (rc 139)
array a=(x y z); echo $(len a)  # → 3   (correct)
```

String length is properly `${#s}` (→ 5). The `BuiltinCmd::Len` doc comment
in `ssa.rs` says "string or array length", which is misleading — the string
path is broken.

## Exposure widened by the len int-typing fix

The shipped fix (adding `len` to `is_int_returning_intrinsic`) makes
`int n=$(len s)` **compile** (before, it was blocked by SB4030) → it now
reaches the garbage runtime path. So closing this hole is more urgent post-fix.
(DG chose 2026-08-04 to ship the int-typing fix and file this hardening as a
separate task.)

## Fix (sketch)

Type-check / validate `len`'s operand: it must be an **array** (declared
`array`/`map`, or an argv/match out-param). A scalar or unknown name →
compile error directing to `${#s}` for a string length. This needs the
operand's type from the `TypeEnv` (or the array-tracking used elsewhere).
Also correct the `ssa.rs` `Len` doc comment to say array-only.

Consider whether `keys`/`join` have the same non-array-operand hole (they're
also DD#41 array ops) and cover them in the same pass.

## Severity

The **segfault** on an undefined operand is a crash (P1). The garbage on a
string is a silent-wrong-answer (also serious). Neither should ever reach
runtime — both are statically detectable (operand is not an array).

## Closed 2026-09-10 — verified done, not merely superseded

DD#75 §2 resolved this by making `len` **type-polymorphic** (Go semantics)
rather than by rejecting the operand, so every original gate is met at HEAD:

| gate | today |
|---|---|
| scalar is not runtime garbage | `len s` on "hello" → `5`, the byte length — deliberate |
| undefined is not a segfault | **SB5039** at compile time |
| `len arr` still works | element count |
| the doc comment | still says "string or array length" — which DD#75 made *true* |

The comment predates the decision but happens to describe current behaviour,
so there is nothing left to correct. Closing.
