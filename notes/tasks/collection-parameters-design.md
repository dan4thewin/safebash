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

## Addendum (2026-09-11) — DG asked about `=`; it is viable, and asking
## uncovered a live bug

DG proposed `=` as the sigil character and observed it *"would only be legal
as an argument"*. Both halves check out, but only after fixing a divergence
the question exposed.

### The bug the question found

`looks_like_assignment` peeked *word* then `Tok::Eq` with **no adjacency
test**. Since the scanner drops whitespace, any command word followed by a
`=`-leading word became an assignment:

| source | bash | sash (before) |
|---|---|---|
| `echo =a` | `=a` | *assigns* `echo=a` |
| `echo =x` then `$echo` | `=x`, then blank | prints `x` |

A silent wrong answer, against the byte-identical-to-bash claim. There was no
spec coverage; `grep =foo f.txt` happened to be caught by SB5011, which is
why it had never surfaced.

### Why the obvious fix is wrong

Requiring plain adjacency regressed **21 tests** (7 × 3 modes) — all DD#86.
Spaced bare assignment is *deliberate*: `fn set_msg { msg = hi }` is the
frictionless-write surface, pinned by `scope.sh` 86.3/86.4/86.6/86.10/
86.14/86.19/86.20. Declarations use it too (`map m = @{p:~}`).

### The rule that works — and why it answers DG

The three shapes are distinct, and only two were claimed:

| shape | spelling | meaning |
|---|---|---|
| glued both | `x=5` | bash assignment |
| spaced both | `msg = hi` | DD#86 frictionless write |
| **spaced left, glued right** | `echo =a` | **unclaimed** |

So `=` binds as an assignment operator iff it is glued on both sides or
spaced on both sides. The asymmetric shape is a plain word — which is
exactly the sigil shape. Implemented in `looks_like_assignment`; full
3-mode sweep **4680 passing / 39 failing / 461 skipped**, zero regressions
(baseline 4669/39/460).

Measured after the fix:

```
echo =a        -> =a          (bash-exact)
f =nums        -> =nums       (ordinary argument)
len =a         -> SB5062      (a word, reaching the operand-spelling check)
unset =a       -> SB5063
x=5, a+=q, msg = hi, map m = @{p:~}   all unchanged
```

### Consequence for the design

DG's "only legal as an argument" is now *structurally* true rather than a
convention: the assignment operator owns the symmetric shapes, and the sigil
owns the asymmetric one. Command-word position is not a name slot in any of
the nine, so nothing is lost.

Two caveats to weigh before choosing `=` over `^`:

- **`[[ ]]` is still asymmetric-hungry.** `[[ $x =b ]]` evaluates as `= b`
  (measured). No name slot lives inside `[[ ]]` today, so it is latent
  rather than live — but a sigil that means one thing outside `[[ ]]` and
  another inside is a readability tax `^` would not levy.
- **Reader load.** `=nums` looks like an assignment fragment; `^nums` looks
  like nothing else in the language. `=` costs no free character, which is
  its real advantage — it was never free, it was *over-claimed*, and the
  fix reclaims it.

## Addendum (2026-09-11) — DG picks `=`; scope of the sigil

DG chose `=`: *"evocative of assigning the collection to the parameter, and
visually similar to `:symbol` without being the same."* Character settled.

DG then scoped it: `=foo` useful as an argument to a user function and *some*
builtins; `read` and `match` keep bare names; `keys` takes `=m` ("symmetry
says bare `m` is a string"); `=` parsing unchanged in `(( ))` and `[[ ]]`.
DG asked whether `read`/`match` are then sore thumbs.

### Verified: `=a` already arrives intact at all nine slots

No ICE, no misparse — each slot rejects with its own correct diagnostic, so
implementation is tractable everywhere:

```
read =ARR SB5057   argv 1 =a SB5063   matchall S P =a SB5063
attest x in =a SB4014   keys =m SB5062   @{=p:~} SB5033
```

**One exception, measured:** `$(( (len =a) ))` → **SB4020** ("an assignment
inside `$(( … ))` can't take effect"). The arith lexer is separate
(`parse/arith.rs`, `b'=' => ATok::Eq`) and owns `=`. So "leave `(( ))`
parsing unchanged" and "sigil in the DD#58 sexpr slot" are in direct
conflict — that slot must either keep bare names as a deliberate carve-out
or the arith lexer must learn the sigil. This is the one place `=` is not
free.

### Answer: yes — but the inconsistency is deeper than `read`/`match`

DG's scoping rests on two *incompatible* principles:

1. **Position**: `read $s a` — `a` is a name because of where it sits. (Used
   to justify bare `read`/`match`.)
2. **Symmetry**: a bare word is a string, so a name needs marking. (Used to
   justify `keys =m`.)

Principle 2 applied to `read` gives `read =a`. Principle 1 applied to `keys`
gives bare `keys m`. Only one can govern. Under (2), `read`/`match` are sore
thumbs; under (1), `keys` is.

**Principle 1 is the one that failed in practice.** It requires the reader —
and the compiler author — to know each builtin's signature. That is exactly
the knowledge that was missing when the language shipped nine name slots
enforced by six unrelated diagnostics, two of them not enforced at all
(SB5062/SB5063, this session). Principle 2 needs no signature knowledge and
can be enforced *once*, in the parser, instead of per-builtin in the
validator.

### DG's own rationale inverts the proposed split

If `=` is chosen because it evokes **binding**, then it fits the slots that
actually bind — the call site, `read`, `argv`, `match`, `unset` — and fits
*least* the pure reads, `len`/`join`/`keys`/`attest`. `read =a` reads as
"read into a"; `len =a` evokes no assignment at all. That is the opposite of
the proposed scoping, which sigils the reads and leaves the writes bare.

This is an argument for uniformity, not for flipping the split: a
semantically-graded rule (writes marked, reads bare) still forces the reader
to know which category a builtin is in — principle 1 again, one level up.

### Recommendation — uniform, with a migration path

Mark **every** name slot, so the rule is "a bare word is a string; `=word` is
a name," with no exceptions to memorise. Churn measured across `spec/*.sh` +
`contrib/`:

| slot | sites |  | slot | sites |
|---|---:|---|---|---:|
| `read`/`readline` | 201 | | `len` | 144 |
| `@{name:DISP}` | 115 | | `match`/`matchall` | 61 |
| `attest … in` | 53 | | `join` | 38 |
| `argv N` | 37 | | `keys` | 23 |
| `unset` | 20 | | `len`/`join` `@a` | 19 |

~692 sites, all mechanical. Staging: accept both spellings, warn on bare,
then make bare an error once the tree is converted.

### `len`/`join` and `@a` — settled, not open

- **`len`**: DG already decided len is **collection-only** with `strlen` for
  scalars. `@a` splats to N *values*, and values are not a collection, so
  `len @a` must be **rejected**. `len =a` is the spelling. This follows from
  the decision already made; it is not a fresh choice.
- **`join`**: collection-only is the well-formed option, on grammar rather
  than taste. The separator is **last**, so a variadic value-list form is
  ambiguous — in `join "x" "y" ","` nothing says whether `","` is an element
  or the separator. A collection operand removes the ambiguity by
  construction. So `join =a ","`, and `join @a ","` rejected.

Both conclusions agree with DG's instinct ("len probably wants `=a`… if we
say join is only for collections then `=a` is better").
