title: DD#100 — Collection parameters: `=name` at the call site, `A`–`E` in the callee
status: accepted

Settled with DG (2026-09-11). sash has no first-class collection values, so a
function could read and mutate a caller's array only by **hard-coding its
name**. Contents were parameterisable (splat `@a`, read-only); identity was
not. This DD adds collection parameters.

## Background — why this was cheap

sash **inlines every function** (DD#27 §5 / SB5037 reject recursion; there is
no runtime call stack). A collection parameter is therefore a **compile-time
name binding** — monomorphisation — and needs *no runtime representation at
all*. That is what made the feature affordable; an earlier estimate that
costed a runtime collection handle was wrong.

## The precedent this copies

`args` is the exact model, already shipped (DD#75 §5 / DD#81): a **reserved,
per-function, compiler-managed collection** where `$1` is sugar for
`${args[0]}`. Measured: `array args=(x)` → **SB5024** at top level,
**SB5052** inside a function. Collection parameters generalise `args` from
"the scalar arguments" to "the collection arguments".

Consequence: **the callee needs no new syntax.** `len A`, `${A[0]}`, `@A`,
`$(join A ,)` and `for x in @A` all work unchanged.

## Part A — the sigil is `=`

At the **call site**, `=name` passes the collection `name`:

```sh
fn show { echo "$1 has $(len A) items: $(join A ,)" }
array nums=(3 1 4)
show "nums" =nums
```

DG chose `=` because it is *evocative of assigning the collection to the
parameter*, and visually similar to a `:symbol` without being the same.

### `=` was not free — it was over-claimed

A leading `=` had been swallowed by the assignment parser: `echo =a` assigned
`echo=a` where bash prints `=a`. Fixed by requiring `=` to bind **only when
symmetric** — glued on both sides (`x=5`) or spaced on both sides (DD#86's
`msg = hi`). The asymmetric shape, spaced-left/glued-right, is a plain word,
which is exactly the sigil shape. See journal 261 and spec `scope.sh`
86.21–86.24.

### It is a *parameter slot*, not a symbol and not a reference

- **Not a symbol.** A symbol is a *value* — storable, passable. If names
  flowed as values, **DD#6's rejection of `${!var}`** re-opens through the
  back door, and every name-based static check (SB5031 undeclared read,
  SB5050 dead variable, taint) becomes undecidable.
- **Not a reference.** A reference drags in an aliasing algebra sash never
  needs: reference-to-reference, outliving the referent, storing one in an
  array. A parameter answers all of those with "no, by construction".
- **A position marker.** So it need not compose — `==a`, `=$x`, `=a[0]` are
  meaningless and rejected.

### Escape hatch

Quoting yields a value, not a name. `f '=foo'` and `f "=foo"` pass the
literal string `=foo`; only a bare `ExprKind::Literal` beginning `=` is a
sigil. The three forms are structurally distinct in the AST
(`Literal` / `SingleQuoted` / `DoubleQuoted`).

## Part B — the callee sees `A`–`E`

- **Five slots, `A` through `E`**, filled in call order.
- **Reserved names**, on the `args` precedent: rejected as user variables at
  top level and inside functions.
- **Array and map are both usable** in any slot. Because every call is
  inlined, each instantiation is monomorphic — the slot's type is whatever
  was passed at that site, and `len`/`keys`/`join` type-check against it
  there.
- A sixth collection argument is a compile error.

## Part C — scalars first, then collections

A call lists its scalar arguments first and its collection arguments last:

```sh
f "name" 3 =items =index
```

`args` is `("name" 3)`; `A` is `items`, `B` is `index`.

Ordering is **not** required for parsing — `=` self-identifies, so
`f =items "name"` would be unambiguous. It is required so that **source
position equals `args` index**: `$1` is the first word, as in bash, without
counting past interleaved collections. That keeps DD#81 Part B's arity
inference (signature from `args` usage, checked at every call site) reading
the way it looks.

DG invited counter-arguments and noted the rule is open. Recorded assessment:
the ergonomic cost is real but small (a verb-subject-modifier call like
`sort_by =items "name"` must be written `sort_by "name" =items`), and the
restriction is **forward-compatible** — interleaving can be permitted later
without breaking any existing call, whereas tightening later would break
them. Starting strict is therefore the safe order.

## Part D — builtins are unaffected

Builtins keep **bare names** and take **no splats** in a name operand
(`len a`, `join a SEP`, `keys m`, `read a`, `unset a`, `argv N a`,
`match S P a`). The sigil is a *call-site* marker, and builtins are not
calls in the relevant sense:

- `attest …` and `array a=(…)` are **language syntax**, not builtin calls —
  they were never operand slots.
- `read a` is **double duty**: it declares `a` *and* populates it. That is
  why it resisted the pattern; it is part declaration, part call.

Removing splats from `len`/`join` is **deferred** into
`dd75-len-collection-only-strlen`, which has to decide `len`'s operand set
anyway — doing it once avoids churning the same 19 sites twice. Note it
collides with SB5025's own advice, which currently recommends `$(join @a)`.

## Related fixes that made this coherent

- **SB5062** — `len`/`join`/`keys` accept only spellings they can resolve.
- **SB5063** — a fill target is a binding, not a value (`argv`, `match`,
  `matchall`, `unset`).
- **SB5064** — `$a` on a collection is an error. It had silently expanded to
  nothing in value contexts while `len $a` resolved it as the collection
  *name* (returning `3` for a three-element array).

## Open

- **Mutation.** Whether a callee may *write* a collection slot (`A[0]=x`,
  `read A`) and have it affect the caller's array, or whether slots are
  read-only. Inlining makes mutation free to implement; the question is
  whether it should be allowed, and how taint and `attest` obligations flow
  across the binding.
- Whether the DD#58 arith sexpr slot (`$(( (len a) ))`) ever needs the
  sigil; today the arith lexer owns `=` and `$(( (len =a) ))` is SB4020.
