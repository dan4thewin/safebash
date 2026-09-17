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

## Part E — a slot *is* the caller's collection (DG 2026-09-11)

`f =foo` binds `A` to the **identical underlying collection**, and `f` may
**modify** it. This is deliberate: it removes any need for out-parameters or
multiple return values — a function can fill several collections in place.

### `=foo` is not a read; `@foo` is

This is the load-bearing distinction, and it falls out of what each form
passes:

| call | passes | is it a read? | attest |
|---|---|---|---|
| `f @foo` | the **values** | **yes** | required at the call site |
| `f =foo` | the **identity** | **no** | **deferred into `f`** |

Measured today, `f @a` on a tainted array is already rejected **SB5020** at
the call site. `=foo` must therefore be excluded from taint's read detection
— that is the one genuinely new rule this DD adds to the validator.

### The rest needs no new machinery

Because every call is inlined, the binding is a **name substitution**, and
the existing passes then see the real sites. All three halves were measured
against the equivalent hard-coded-name program:

- **Taint flows in.** A tainted collection read inside a function body is
  rejected **SB5020**, exactly as at top level.
- **`attest` inside the function discharges it.** `fn f { attest e in a
  [[ -n $e ]]; echo ${a[0]} }` is accepted — so DG's "deferred into `f`" is
  already the behaviour, not a new mechanism.
- **Mutation and write-back already work.** A function writing `a[0]=z`
  changes the caller's array; and a write from a tainted source inside the
  function taints it, so the caller's later read is rejected **SB5021**.

So Part E is mostly *binding plus one exclusion*, not a new taint model.

### Consequences to expect

- **Diagnostics can point inside a callee because of a caller.** Safety is
  per-instantiation, so a function that is fine at one call site can raise a
  taint error at another. The error names a line inside `f` whose cause is a
  call elsewhere; the message should say which call site instantiated it.
- **Aliasing two slots to one collection** (`f =foo =foo`) is coherent under
  substitution — writes through `A` are visible through `B`. Allowed, but
  worth a spec pinning it.
- **`=foo` alone does not satisfy SB5050.** If `f` only *writes* the slot,
  `foo` is still never read, and the dead-variable check fires unless the
  caller reads it afterwards — which is the correct shape for an out-param.
- **`=foo` on a scalar, or on an undeclared name, is an error** (slots are
  collections; SB5031 already covers the undeclared case).

## Part F — what implementation settled (2026-09-11)

Implemented across `validate.rs` (SB5065/SB5066/SB5067), `type_check.rs`
(slot kinds), the transform (binding + substitution) and `spec/collparam.sh`
(34 tests). Five things the design did not decide, decided by building it:

### The sigil arrives as a `Concat`, not a `Literal`

The scanner emits `=` as its own token, so `f =nums` reaches the AST as
`Concat([Literal("="), Literal("nums")])`. `collection_arg_name` accepts a
`Literal` **or** an all-`Literal` `Concat`. This is fortunate rather than
awkward: a non-literal part (`=$x`, `=@a`) fails naturally, and the escape
hatch still works, because `f "=foo"` is a `DoubleQuoted` — structurally
distinct, exactly as Part A assumed.

### A slot's kind comes from its call sites, and they must agree

`type_check` runs **before** inlining, so a slot has no program-wide kind and
the kind-specific checks had nothing to check against. Skipping them for slots
was measured and rejected: `keys A` on an array printed **garbage** and
`${A[x]}` on an array **silently returned element 0** — two wrong answers the
project had already promoted to compile errors (DD#75 §3, DD#80).

So each function's slot kinds are inferred from its call sites, and call sites
that **disagree** are rejected (SB4030). This narrows Part B, which allows any
kind in any slot: a genuinely kind-agnostic helper (only `len A`, `@A`,
`join A`) called with an array at one site and a map at another is now an
error. Strict-first for the same reason Part C gives — per-instantiation
checking can be allowed later without breaking an accepted program, whereas
tightening later would break them. (DG confirmed, 2026-09-11.)

### `=foo` *is* a read when the callee reads the slot

Part E says `=foo` passes identity, not a value, so it is not a read. Applied
literally, SB5050 rejected **Part A's own example** — `show "nums" =nums` saw
only the `=nums` token and called `nums` dead. Resolved with slot-usage
analysis: `=foo` counts as a read of `foo` exactly when the callee reads that
slot. Writes are deliberately *not* counted, so the out-parameter-never-read
case still fires, which is what Part E asks for.

### Substitution has to cover every write path, not just reads

`resolve_args_name` is the single substitution hook, but it was only wired
into reads. Three write paths bypassed it, each found by probing rather than
by reading the code:

- `handle_subscript_assign` → **ICE** (`undefined array A`) on `A[0]=z`
- the five bulk-fill sites → `read A` silently filled a function-*local* `A`
  and left the caller's collection untouched
- both attest-all sites → **ICE** (`undefined attest-all array: A`), which is
  precisely the deferral Part E exists for

Any new path that names a collection must call `resolve_args_name` or
`fill_target_name`.

### Slot taint crosses the boundary in both directions

Part E's "a slot is the caller's collection" makes taint a call-boundary
problem, and neither direction was covered:

- **inward** — `argv a; f =a` let `${A[0]}` reach `echo` unattested
- **outward** — `fn fill { argv A }` left the caller's `a` tainted-but-clean

Both are now resolved structurally before the walk (`analyze_slots`), with a
fixpoint over the call graph so forwarding (`fn g { f =A }`) closes too. DG
chose to **allow** a callee to fill a slot from outside and propagate the
taint outward, rather than ban it as SB5067 bans the global case: an
out-parameter carrying external data is much of what out-parameters are for.

Inward taint is a **union over call sites** — a function called once with a
tainted collection and once with a clean one is checked as tainted. That
matches the slot-kind rule, where all call sites must agree too.

This work also uncovered and fixed a **pre-existing** taint bypass unrelated
to slots; see SB5067 in DD#24's lineage and `notes/journal/`.

## Clarification (DG, 2026-09-11): a bare name to a user function stays a string

**Ruling.**

> I'm going to just live with the current state that non-sigiled [names]
> provided to a user function are strings, since there is no expectation that
> they should operate on bare variables/collections, unlike `len`, `join`, et
> al. With use, I may reconsider or have an inspiration.

This closes `bare-collection-name-operand-asymmetry`, which asked whether a
bare collection name passed to a user function should be **rejected**,
directing the author to `=a` (the collection) or `"a"` (the literal).

The asymmetry it reported is real and measured (2026-09-11):

```sh
array a=(x y z)
show() { echo "arg1=${args[0]}"; }
show a              # arg1=a      -- the STRING "a"
echo "len=$(len a)" # len=3       -- the COLLECTION a
```

One bare word, two meanings, in one program, with no diagnostic. It is
nevertheless **not** a defect, because the two callees are different kinds of
thing:

- `len` / `join` / `keys` are **language surface**. Their operand slot is
  declared to name a collection; that is what the builtin is *for*, so a bare
  name there can only sensibly be a name.
- A **user function** takes values. Nothing about it creates an expectation
  that a bare word should be resolved as a variable or a collection, any more
  than `f hello` should look for a variable named `hello`.

So the rule is regular once the two are distinguished: **bare word to a user
function = string, always.** `=name` is the one spelling that binds a
collection, and it is unambiguous by construction.

Marked as a "live with it" rather than a permanent settlement — DG left the
door open to reconsider with use. Pinned by `spec/collparam.sh` 100.35–100.37
so the behaviour cannot drift silently in the meantime.

## Open

- Whether the DD#58 arith sexpr slot (`$(( (len a) ))`) ever needs the
  sigil; today the arith lexer owns `=` and `$(( (len =a) ))` is SB4020.
- The janet backend has no slot support (`notes/tasks/janet-collection-
  parameters.md`); the 17 runtime tests in `spec/collparam.sh` are guarded
  `in_modes native,c`.
- A function defined *inside top-level control flow* keeps the old
  definition-point taint walk (`notes/tasks/taint-fn-in-toplevel-control-
  flow.md`).

## Addendum (2026-09-17) — Part B's reservation is now enforced for `for` variables

Part B states that `A`–`E` are "reserved names, on the `args` precedent:
rejected as user variables at top level and inside functions". That was prose
only: nothing rejected a **`for` variable** taking a slot name, because a loop
variable falls between the two gates that would otherwise catch it — DD#86
explicitly exempts a for variable from the shadowing rule ("a `for` variable
is not a declaration and is untouched"), and SB5052 gates *writes*
(assignments, declarations, bulk fills), which a loop variable is not.

Inside a function that gave one name two simultaneous meanings: `$A` read the
loop variable while `len A`, `join A`, and `${A[0]}` read the slot.

```sh
fn f { for A in @B { echo "$A -> $(len A)" } }   # was: xx -> 3
```

**SB5080** now rejects a `for` variable named with a slot letter (`A`–`E`) or
`args`. At top level the collision was already SB5065. Lowercase names that
merely resemble a slot (`for e in …`) remain legal — the slot namespace is
single uppercase letters only.

Specs `spec/control_flow.sh` cf.n14–cf.n16;
`notes/tasks/bug-reserved-name-as-loop-var.md`.

## Addendum (2026-09-17) — Part D is now backed by ICE guards

Part D removed the splat operand from the name-taking builtins, so `len @a`
and `join @a SEP` are SB5062 (specs `75.69`/`75.70`). That left the
`ParameterExpansion(arrvals)` arm in both `try_len_intrinsic` and
`join_intrinsic` unreachable from any accepted program.

Both arms are kept as `unreachable!` guards rather than deleted, because the
fallback each one shadows degrades **silently**. `expr_to_string` renders a
parameter expansion as `${NAME}{OP}`, so `@a` becomes the string
`"${aarrvals}"`; `len` strips the `$` to `"{aarrvals}"`, misses
`state.collections` (which holds `"a"`), and emits `strlen` of an undefined
var — **0 where the element count belongs**. `join` joins a name no program can
declare. A regression in SB5062 would therefore produce a wrong answer, not a
crash. Unit tests in `transform/expr_handlers.rs` simulate the regression and
assert the panic names SB5062; a third test pins the silent-degradation shape
itself, so if the fallback ever fails loudly the guards can be reconsidered.

(An earlier version of this addendum said the fallback yields the literal text
`"@a"`. That was asserted from reading and is wrong — corrected by execution.)

The splat itself is unaffected: `for x in @a` (`control_flow.rs:574`) and `@a`
as a command argument (`expressions.rs:103`) are live. Part D scoped the
removal to the *operand of a name-taking builtin*.

Journal 356.
