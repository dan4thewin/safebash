title: "BUG: const reassignment panics the compiler (ssa.rs:926) instead of a clean diagnostic"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`const x=1; x=2` produces a clean SBxxxx compile error (not a Rust `thread 'main' panicked` / rc 101)"
  - "all reassignment paths of a const are covered: plain `x=2`, compound `x+=1`, arith `(( x=5 ))` / `(( x++ ))`, and re-`const x=2`"
  - "a spec pins const-reassignment → the SB code (errors.sh)"

## RESOLVED (2026, fork C — DG-approved)

Fixed with **SB4044** both ways (defense-in-depth):
- **B (validate.rs)** `check_const_reassignment`: a flat pass collecting
  `const`/`readonly` names and flagging plain/append/indexed assignment,
  re-declaration, and arith `(( ))` mutation (`x=`, `x++`, `x+=`, `a[i]=`) of a
  const — **spanned** diagnostic.
- **A (ssa.rs)** `VersionState::define`: the `panic!` is replaced by a spanless
  `SB4044` pushed to `state.diagnostics` (fall through, no panic) — a backstop
  for any channel B doesn't enumerate (`read`/`mapfile` into a const both
  verified to hit it).

Specs `spec/types.sh` 35.3-35.9 (plain, `+=`, `(( x++ ))`, `(( x=5 ))`,
re-`const`, `read` backstop, and the no-false-positive case) — 21 new passing
across native/c/janet, 0 regressions. Gates met.

Known adjacent edge (NOT this bug): `argv [0] x` into a const isn't blocked by
SB4044 — but it independently runtime-aborts via the intrinsic-arity fallthrough
(`notes/tasks/bug-intrinsic-arity-fallthrough.md`), so it's entangled with that
bug, not a silent const violation.

## Symptom (found via the coverage audit, 2026-08-28)

```
$ ./sash -c 'const x=1; x=2; echo $x'
thread 'main' (…) panicked at sash/src/ssa.rs:926:13:
sash: x: readonly variable — cannot reassign
```

rc=101. A user reassigning a `const` gets a **compiler panic**, not a clean
diagnostic. This is a reachable `panic!` on ordinary (parseable) input.

## Cause

`VersionState::define` (ssa.rs:923) enforces readonly by `panic!` at
ssa.rs:926. `const` maps to `DeclKeyword::Readonly`, which
`transform/statements.rs:472` records in `readonly_vars`. **Validate has NO
const-reassignment check**, so `const x=1; x=2` passes validation, reaches the
transform, and the second `define("x")` panics.

`define()` is called from *every* mutation path (plain assignment, compound
assignment, arith `(( ))` mutation, read-into-var, …), so the panic is
reachable many ways.

## Fix — approach fork (present to DG)

- **A. At the `define()` site** — change the `panic!` to push a diagnostic to
  `state.diagnostics` and continue. Pro: one change catches ALL mutation paths;
  eliminates the panic completely. Con: no precise span (define() takes only a
  name) — a spanless / `1:1` diagnostic, unlike sash's usual spanned errors.
- **B. A validate-time pass** — collect `const` names, reject any assignment /
  compound-assign / arith-mutation targeting them with the assignment's span.
  Pro: proper spanned diagnostic. Con: must enumerate every mutation form or a
  missed one still panics; more code.
- **C. Both** — B for the common `x=…` form (nice span) + A as the backstop
  (kills the panic for any form B misses).

Needs an SB code (next free ~SB4044). No prior DD/task on const-reassignment
found.

## Note

Found while auditing coverage (the
[historical coverage ledger](https://github.com/dan4thewin/safebash/blob/f02477dd31e67a0fe6fe3c4a359935569c4715d8/notes/coverage-accounting.md)):
ssa.rs:926 was a "missed" line; testing it revealed the panic. Exactly the kind
of reachable-panic the audit is meant to surface.
