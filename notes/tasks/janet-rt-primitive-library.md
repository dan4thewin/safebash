---
title: "EFFORT (architecture): janet backend = thin IR lowering + `sash-rt.janet` primitive library"
status: ready
priority: P3
kind: effort
depends: []
supersedes-strategy: effort-janet-backend-parity
gates:
  - "a `sash-rt.janet` library provides the sash-rt primitive surface natively (no `os/spawn` of fake `getpid`/`read`/`regex` commands); emit_janet emits thin CALLS into it, not inline re-implementations"
  - "emit_janet keeps lowering the IR's STRUCTURED control flow (if/while/for/case/pipeline) itself — control flow is NOT delegated to the library (that independence is the janet oracle's whole point)"
  - "the library is independently unit-testable in janet; the native-vs-janet parity gate (janet-parity-gate) stays green"
  - "the janet failing-spec count ratchets down as each primitive family lands + its guarded specs un-skip; bundled sub-tasks (effort-janet-backend-parity) fold into this"
---

# Decision (2026-08-16, DG)

Rethink of the janet oracle. DG's framing: "could the compiler emit an IR
representation … that carries no additional janet code but assumes janet
provides all the primitives native does, then write janet libraries that
provide those primitives?"

**Resolved: the "idealized janet" reading — a `sash-rt.janet` primitive library
+ thin leaf lowering, keeping the IR-based control flow.** NOT the
"assembly-like / SSA-retarget" reading.

## Why this shape (and not SSA/assembly)

Two backends already sit at two IR levels *by design* (see the resolved
`emit-janet-from-ssa`, DG 2026-06-06):

| backend | lowers from | primitives | role |
|---------|-------------|-----------|------|
| native  | SSA         | `libsash_rt.a` (Cranelift calls) | product |
| **c**   | **SSA**     | **`libsash_rt.a`** (thin, goto+labels) | **SSA-stage** oracle |
| janet   | **IR/AST**  | *re-implemented inline in janet* | **IR-stage** oracle |

emit_c is ALREADY "thin lowering + shared runtime" at the SSA level — that is
the c oracle's near-parity (13 fail vs native 0). The obvious "make janet like
c" = rebase janet on SSA — was considered and **rejected** in 2026-06 for two
reasons that still hold:

1. **No `goto` in janet.** SSA is a flattened CFG; c uses `goto`+labels. A
   faithful SSA→janet needs a relooper/structurizer (wasmtime-style) or a
   block-function trampoline → a whole new pass + unreadable janet.
2. **Oracle independence.** janet lowering from the IR re-derives control
   structure independently of the transform, so it can catch IR→SSA (transform)
   bugs that c/native — living below the transform — cannot. Rebasing janet on
   SSA collapses it into a redundant second copy of the c oracle.

DG's proposal does NOT require going to SSA. Its core insight — "no additional
janet code; janet provides the primitives" — applies at the **leaf** layer,
where janet actually diverges. Keep the IR-based structured control flow
(independence preserved, no relooper); replace every inline primitive
re-implementation with a call into `sash-rt.janet`.

## What diverges today (the 36 janet failures)

Overwhelmingly **leaf primitives**, not control flow:
- `os/spawn`-exec of intrinsics that native does in-process: getpid/getppid/
  getcwd/getenv/epochseconds/random/argc (`janet-getpid-class-intrinsics`),
  read/mapfile, regex (`sash-regex` exec).
- inline re-derivations: the `_num` arith coercion (preamble), `scan-number`
  string→int, DD#25 overflow (`bug-janet-int-overflow`), len/keys/join
  (`janet-join-intrinsic-gaps`), status/`$?` in arith
  (`bug-janet-special-var-arith`), signum (`c-janet-signum-intrinsic`).
- DD#56 signals — the single biggest cluster (~14 of 36): signal/trap/signum/
  signal-flag as janet primitives.
- DD#55 abort channel, predicate `!` in condition
  (`janet-predicate-function-gaps`), DD#53 zero value
  (`janet-scalar-zero-value`).

A handful (`cf.f9/f10/f12`, `cf.i2`) are genuine control-flow divergences —
those are emit_janet IR-lowering bugs, fixed on their own (NOT via the library);
they are exactly the signal the IR-stage independence is meant to surface.

## `sash-rt.janet` — primitive surface (mirror `libsash_rt.a`)

Delivered as a preamble/bundled library (the current `_num` + `__last_exit`
preamble is the seed) OR an `(import)` the harness resolves.

- **Introspection/process:** `getpid getppid getcpid getcwd getenv epochseconds
  random argc arg status` — native janet (`os/…`, ffi/syscall where needed), not
  exec.
- **Numeric:** `atoi` (the `_num` bash-coercion: leading int, 0x/0o/0b, sign),
  `itoa`, checked `add/sub/mul` (DD#25 trap), `div/mod` (÷0), `pow`, `signum`,
  `dtoi/itod` (DD#36 fixed-point).
- **Collections:** table `create/get/set/delete`, `len` (arrlen/strlen
  dispatch), `keys`, `join` (IFS), indexed-array + dynamic subscript read/assign.
- **Strings:** `toupper/tolower`, `basename/dirname`, `sub/gsub/match` (janet PEG
  or a shared regex, not `sash-regex` exec).
- **IO:** `read/mapfile` (from fd/pipe, not exec), `echo/printf`, redirects. Real
  external commands still `os/spawn` (correct — that is not a primitive).
- **Signals (DD#56):** `signal trap signum signal-flag` + the check-point model.
- **Control-semantics primitives:** DD#55 abort channel, DD#53 typed-decl zero
  value (no env fallback).

**Boundary (do NOT delegate):** if/while/for/case, pipeline *structure*,
function inlining, condition lowering — emit_janet keeps emitting these from the
IR. Only value/effect **leaves** call the library.

## Migration (incremental, ratchet)

One primitive family per slice, smallest/cleanest first (getpid-class is already
scoped in `janet-getpid-class-intrinsics`). Each slice: add the family to
`sash-rt.janet`, switch emit_janet's leaf emission to call it, un-guard the
family's specs for janet, confirm the parity gate + full sweep, ratchet the
janet fail count down. `effort-janet-backend-parity`'s bundled sub-tasks
(getpid-class, predicate-gaps, scalar-zero) fold in as slices.

## Wins

- Kills the leaf noise (~30 of 36) → janet becomes a trustworthy IR-stage oracle
  instead of a per-test-guarded liability.
- `sash-rt.janet` is unit-testable in isolation (feed inputs, assert outputs) —
  impossible with today's inline emission.
- Preserves the transform-independence that is janet's reason to exist.
- No relooper, no new compiler stage, IR control flow untouched.

## Open (later, if ever)

The "assembly-like / SSA target-neutral IR" reading (a canonical sash-IR both
backends consume via runtimes) stays a separately-decided future option — only
if janet's purpose shifts from oracle to portable execution target. Not now.
