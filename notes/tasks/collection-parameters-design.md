---
title: Collection parameters and the name sigil — symbol or reference?
status: open
priority: P2
kind: decision
depends: []
gates:
  - DG rules on symbol-vs-reference framing
  - DG rules on call-site marking (the remaining gap)
  - sigil character and placement chosen against the nine-slot inventory
---

DG (2026-09-11) proposed passing collections to functions through a distinct
reserved set — `A`, `B`, `C`… — parallel to the scalar `$1`, `$2`: `A` is the
array/map, `${A[0]}` its first element. DG called this *experimental
territory, not immutable design*, flagged that the no-shadowing rule may
pertain, and asked whether the sigil is **more like a Lisp symbol or a
reference**.

## The answer: neither — it is a *parameter slot*, and sash already has one

`args` is the precedent, and it is exact. Per DD#75 §5 / DD#81, `args` is a
**reserved, per-function, compiler-managed collection**: `@args` splats,
`${args[i]}` indexes, `$(len args)` counts, and `$1` is pure sugar for
`${args[0]}`. It is reserved with its own diagnostics — measured:

| written | result |
|---|---|
| `array args=(x)` at top level | **SB5024** |
| `array args=(z)` inside a fn | **SB5052** |

DG's `A, B, C` is not a new mechanism. It is `args` generalised from "the
scalar arguments" to "the collection arguments". That is the proposal's main
strength and the reason it sidesteps the sigil question on the callee side
entirely: `A` is an ordinary name in the callee's namespace, so `len A`,
`${A[0]}`, `for x in @A` and `$(join A ,)` all work with no new syntax.

### Why "symbol" is ruled out by existing decisions, not by taste

A Lisp symbol is a **value** — storable, passable, comparable. If names could
flow as values, then `s=^a; len $s` must mean something, and that re-opens
**DD#6's rejection of `${!var}`** indirect expansion through the back door.
Worse, it would make every name-based static check undecidable: SB5031
(undeclared read), SB5050 (dead variable), and the whole taint analysis all
depend on knowing statically *which* binding a site touches.

### Why "reference" is also not quite right

A reference is a value denoting a location, which forces answers to an
aliasing algebra sash does not otherwise need: reference-to-reference,
outliving the referent, storing one in an array. A **parameter** answers all
of those with "no, by construction" — the binding exists only for the
duration of the call and only in the callee's namespace.

And because sash **inlines every function** (DD#27 §5 / SB5037 reject
recursion; there is no runtime call stack), a collection parameter is a
compile-time name binding — monomorphisation. It needs **no runtime
representation at all**. Taint follows the referent; each instantiation sees
one concrete name.

### The three uses, distinguished

| use | what it is | closest analogue | runtime existence |
|---|---|---|---|
| `len a`, `read ARR`, `unset a` | unevaluated operand position | Lisp special-form arg / C lvalue context | none — pure syntax |
| `f nums` → `A` | parameter binding | `args`, already shipped | none — inlined substitution |
| `s=^a; len $s` | symbol as a value | Lisp `'foo`, bash `${!var}` | would need runtime name→binding |

Row 3 is what DD#6 forbids. Sash's answer is rows 1 and 2, never row 3.

### The consequence for the sigil

If a name is never a value, the sigil is **not a type constructor — it is a
position marker**. Three things follow:

- It need not compose. `^^a`, `^$x` are meaningless and can be rejected.
- It can be rejected *everywhere* except declared name positions, which turns
  the current "nine slots, six diagnostics, three ad-hoc implementations"
  problem into a single parser rule.
- Under the `A, B, C` scheme it is **never needed in the callee**, because the
  callee uses an ordinary reserved name. That answers the placement question
  by construction rather than by preference: **call site only**.

## The remaining gap — call-site marking

`A, B, C` solves the callee side cleanly and leaves the caller side untouched.
`f nums` is still ambiguous between the collection `nums` and the string
`"nums"` — which is precisely `bare-collection-name-operand-asymmetry`
relocated, not resolved. Closing it needs either a call-site sigil (`f ^nums`)
or a declared signature. This is the one place the proposal does not stand on
its own.

## No-shadowing interaction (DG's flag was right)

DD#86 forbids a local hiding a same-named global (SB5049, `check_no_shadow`).
Reserving `A, B, C` means a user global named `A` would be shadowed inside any
function taking a collection parameter — exactly the forbidden shape. The
resolution is the `args` precedent: make them **reserved names**, rejected as
user variables, rather than letting the collision arise.

Measured cost of reserving them — single-uppercase names in real sash sources
(excluding generated `spec/.trace/` artifacts and the external `suites/oils`
bash corpus):

    2 × `V=`   spec/coverage.sh
    1 × `A=`   spec/taint.sh

Three uses, two files. Reserving `A, B, C` is essentially free today.

## Still open

- **Call-site marking** — the gap above; the only load-bearing unknown.
- **Max arity** implied by a fixed letter set, and how scalar and collection
  arguments interleave in the call.
- **Array-vs-map typing of `A`** — monomorphisation makes per-instantiation
  typing natural, but it should be stated.
- **Sigil character.** Measured free leading characters, each currently
  lexing as an ordinary word (so any choice costs churn): `:` `%` `^` `~` `+`.
  Taken: `&` `=` `@` `#` `*` `?` `'`. `:` fits semantically (Janet backend;
  Clojure/Ruby `:sym`) but collides with the `:` builtin and the `@{a:DISP}`
  separator. `^` is unused anywhere in the language today.
- **"Or more than one"** — whether DG meant in/out direction. Position: one
  sigil suffices, since direction is a property of the callee the compiler
  already knows.
