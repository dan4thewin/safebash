---
title: "janet: scalar zero-value semantics diverge (typed decl no-init; bare-scalar env-fallback)"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "janet `int n; echo $n` → `0` (typed scalar decl zero-inits), not `(var n )` (empty value = janet syntax error)"
  - "janet a KNOWN-but-unassigned scalar read → the meaningful zero value (\"\" untyped / 0 int), NOT `(os/getenv NAME)` env-fallback — a plain sash scalar is not an env var (DD#53; env is only via `$(getenv NAME)`)"
  - "un-guard spec/undeclared.sh ud.5, ud.6, ud.11 to include janet once both are fixed"
---

# janet scalar zero-value semantics diverge from native/c

## Problem

Two janet-backend divergences surfaced while adding DD#62 Model B specs
(`spec/undeclared.sh`). Both concern the **zero value** of a scalar that
is known-in-scope but not yet assigned. Native and c produce the
meaningful zero (untyped→`""`, int→`0`); janet does not.

### 1. Typed decl without initializer → empty janet value

```
int n; echo $n
```

emits:

```janet
(var n )        ; <- empty value, janet syntax error
(print (string "[" n "]"))
```

Native/c zero-init `n` to `0`. janet should emit `(var n 0)` for an
`int` decl with no initializer (and `""` for an untyped decl).

### 2. Bare scalar read falls back to the environment

```
echo [$x]; x=5
```

emits:

```janet
(print (string "[" (or (os/getenv "x") "") "]"))
```

A **plain sash scalar** must never read from the process environment.
Per DD#53 the environment is reachable only through `$(getenv NAME)`;
a bare `$x` is a sash variable whose unset value is `""`. The
`(or (os/getenv "x") "")` fallback happens to yield `""` only when the
host env lacks a var of that name — fragile and semantically wrong.
janet should reference the sash binding directly (its zero value if
unassigned), not `os/getenv`.

## Why native/c are unaffected

Both oracles model scalars with a definite storage slot initialized to
the type's zero value; there is no env fallback for a bare scalar read.
The DD#62 Model B front-end check (SB5031) is backend-independent and
passes all three modes — this is purely a janet **codegen** divergence.

## Scope

- `sash/src/emit_janet.rs` — scalar decl emission (zero-init typed/untyped),
  and bare-scalar read emission (drop the `os/getenv` fallback).
- After fixing: remove the `in_modes native,c` guards on
  `spec/undeclared.sh` ud.5, ud.6, ud.11.

janet-is-illustration: this is not a gate, but the zero-value semantics
are simple and worth aligning so the emitted janet reads faithfully.
