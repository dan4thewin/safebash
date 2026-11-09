title: DD#100 — Collection parameters: `=name` at the call site, `A`–`E` in the callee
status: accepted

Settled with DG (2026-09-11). sash has no first-class collection values, so a
function could read and mutate a caller's array only by **hard-coding its
name**. Contents were parameterisable (splat `@a`, read-only); identity was
not. This DD adds collection parameters.

**Current rule (DG, 2026-10-18): one inferred signature per function.**
Collection count, array/map kind and string/int element type must agree at
every call. The addendum below supersedes Part B's per-instantiation typing
and Part F's suggestion that polymorphic checking might be added later.

## Addendum (2026-10-18, DG) — one function, one inferred signature

> "while we do infer the signature of a function, I don't think we can allow
> a function to have more than one signature"

Each function has a fixed collection-argument count and one full type for
each slot: **string array, int array, string map, or int map**. Even a helper
whose body only uses `len A` cannot be called with two different types, or
with different numbers of collection arguments. DD#81's fixed/variadic
scalar-argument rules are unchanged.

Forwarding (`inner =A`) propagates the same signature through the call graph.
Inference reaches a fixpoint before reporting disagreements as SB4030.
Every type-checking visitor enters the inferred slot environment, including
arithmetic, string operands, defaults and redirect checks. A string cell
through `A` therefore owes precisely the conversion an ordinary string cell
owes; an int collection cannot receive a string-producing bulk fill.

This fixes measured bypasses, not just a diagnostic gap: a string cell
containing `nonsense` evaluated to zero in arithmetic, and a string assigned
through an int slot reached native codegen's non-integer-literal ICE.
An uncalled function has no concrete call-site signature; unknown slots
remain conservative rather than inventing a type.

Specs `spec/collparam.sh` `slotty.1`-`slotty.40`; journal 525.

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
**modify** a mutable actual. This is deliberate: it removes any need for
out-parameters or multiple return values — a function can fill several
collections in place.

DD131's provisional immutable-scratch trial preserves this identity
and the actual collection's immutability. A read-only helper may accept
scratch, but writes, fills, append and unset must reject, including
through transitive forwarding. Mutable A-E targets accumulate under
the proposed fill policy, independent of the caller's original name.
A fresh-binding request cannot target an already-bound slot.

DD132 additionally requires real regex proof when a received element
serves as a pattern; cleanliness or identity alone is not validation.
No call may launder a trusted read into proof or export a caught
scratch attest's limited permission. Non-aliasing, element/kind typing
and the one-signature rule remain; the integrated trial owns enforcement.

### `=foo` is not a read; `@foo` is

This is the load-bearing distinction, and it falls out of what each form
passes:

| call | passes | is it a read? | taint |
|---|---|---|---|
| `f @foo` | the **values** | **yes** | a tainted `foo` is **SB5020** at the call |
| `f =foo` | the **identity** | **no** | decided by what `f` does with the slot (DD#127) |

`f @a` on a tainted array is rejected **SB5020** at the call site. `=foo` is
excluded from taint's read detection, but still obeys the non-aliasing
contract below.

### Non-aliasing contract (DG, 2026-11-02)

**Every collection argument has a distinct identity, and a collection passed
through a slot is accessed only through that slot or a forwarded slot for
the duration of the call.**

- `f =foo =foo` is rejected, even if both slots are only read.
- If `f` receives `=foo`, neither `f` nor a function it calls may access
  `foo` by its global name. The restriction follows the whole call chain,
  including calls in captures, arithmetic, pipeline stages and background
  bodies. Extracting a helper does not evade it.
- Access includes reads, writes, `unset`, `attest`, metadata operations such
  as `len`, and passing `=foo` to another function. Forward `=A` instead.
  A builtin or external command's literal word, such as `/bin/echo =foo`,
  is not a collection access.
- The rule is static: a use in a conditional or unreachable branch counts.
  There are no read-only or taint-inert exceptions, and `--no-taint` does not
  disable the contract.
- Other globals remain available. A function that reads global `foo` may
  receive a different collection through A. The caller may use `foo` again
  after the call.

Both violations are **SB5099** at the call. A global-overlap diagnostic also
names a function and statement that accesses the global. Forwarding carries
the restriction outward until the actual collection identity is known.

This reverses the earlier permission for duplicate slots; it does **not**
make slots copies or remove in-place mutation. It is the prerequisite for
DD#127's one-summary-per-function slot analysis: the reviewer and checker
need not reconstruct alias relationships for each caller. Non-aliasing is
not, on its own, a taint-safety guarantee; source tracking, read/attest
checks, summary composition and control-flow handling still supply that
protection. The strict contract is simpler than alias-specialised or
alias-parametric summaries; it is not a claim that the whole implementation
is smaller than the former call-context walker.

**Enforcement (journal 558).** `check_collection_args` runs on resolved
lexical identities, outside the optional taint check. It rejects duplicate
arguments and intersects the passed identities with the callee's global-use
footprint. The shared read/binding walks collect the footprint; identity
forwarding and the canonical `unset` parser cover uses that need not read a
value. The existing command/arithmetic call graph supplies the transitive
closure, with a body location retained for each global. Multiple definitions
of a function conservatively contribute to the same footprint. This
prerequisite landed before the slot summaries (journal 559).

### Taint through a slot

In the transform, each inlined call substitutes the caller's collection
wherever `f` names its slot. The pre-inlining taint check uses that same
identity relation when applying a summary. What taint may cross the call is
DD#127's rule, applied to a slot like any other name:

- **Inward.** A slot that arrives tainted carries taint `f` received. `f` may
  read it with `:!` (DD#91). An unescaped value use is rejected at the call,
  with the code the statement would raise if the taint were `f`'s own:
  **SB5020** for a read, **SB5021** for writing it into a collection cell,
  **SB5088** for an attest. The caller attests before the call. Length is
  metadata, not a tainted value read: `len A` retains the direct collection's
  behavior.
- **Outward.** A slot `f` fills (`fn fill { argv A }`) carries `f`'s own
  taint, and `f` may attest it adjacently, inside itself. If it does not, and
  `f` has exactly one call site, the caller attests immediately after the
  call, or a later non-`:!` read is **SB5020**. A function with zero, two or
  more call sites may not leave the slot tainted: **SB5067** at the statement
  that filled it.
- **A tainted value written into a cell** is **SB5021** at the write
  (DD#61), wherever the value came from.
- **Mutation and write-back need nothing new.** A function writing `A[0]=z`
  changes the caller's array.

The slot supplies the collection's identity; DD#127 supplies the taint
model. Globals and slots now share the same independent summaries,
described below. DD#127's "Implementation status" records the remaining
stdout, control-flow and SSA limitations.

### Consequences to expect

- **A caller's taint is reported at the call.** Each body is summarised
  independently, so its own verdicts never depend on its callers. When a call
  passes in taint that `f` would use, the error is reported at the call and
  names the line in `f` that uses it (DD#127).
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
- both attest-all sites → **ICE** (`undefined attest-all array: A`) on
  `attest v in A`, the form a body needs to attest a slot it filled (DD#127)

Any new path that names a collection must call `resolve_args_name` or
`fill_target_name`.

### Slot taint crosses the boundary in both directions

Part E's "a slot is the caller's collection" makes taint a call-boundary
problem, and at first neither direction was covered:

- **inward** — `argv a; f =a` let `${A[0]}` reach `echo` unattested
- **outward** — `fn fill { argv A }` left the caller's `a` tainted-but-clean

"Taint through a slot" above closes both. DG's 2026-09-11 decision to
**allow** a callee to fill a slot from outside and pass the taint out stands
for a function with one call site, whose caller attests after the call: an
out-parameter carrying external data is much of what out-parameters are for.
A function with zero, two or more call sites attests what it fills, exactly
as it would for a global (DD#127).

The implementation uses one summary per function over globals and symbolic
slots A-E (journal 559; DD#127, "How it is checked"). Empty-entry walks find
own taint; singleton inputs find flows and conditional diagnostics. At
`f =a =b`, A maps to `a` and B to `b`; forwarding composes those mappings,
including permutations. A call is checked against the taint arriving there,
not the final top-level state or a union of its other callers.

A passed global is not also mapped through its unused global identity:
SB5099 forbids the callee from accessing that path, and retaining its
identity flow would undo cleanup through the slot. An in-body fill/attest
therefore clears the caller's collection at a statement call (B115).
Read-before-cleanup obligations remain separate from the outgoing flow.
Received taint is never freshly adjacent at body entry, so an inherited
attest is SB5088 at the actual call.

The old `SlotInfo`, `analyze_slots`, inward-entry union and outward-fill
fixpoint are removed entirely. `slot_usage_map` remains for collection arity
and dead-variable checks; it does not supply taint. Type-signature inference
and transform name substitution are unchanged.

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

## Addendum (2026-09-28) — `=name` reached the slot on one call path in four

Part F ends with "Substitution has to cover every write path". The same is true
of *call* paths, and it was not: `split_call_args` — the function that peels
`=name` arguments off a call and binds them to `A`–`E` — had exactly **one**
call site, the plain-statement path. The other three lowerings treated `=q` as
an ordinary scalar argument, so the slot was never bound and argc was off by
one.

| call position | before |
|---|---|
| `f =q` | correct |
| `f =q \| g` | `len A=0`, `len args=1` — a silent wrong answer |
| `str t=$(f =q)` | **SIGSEGV** at run time |
| `t=$(f =q)`, `echo "$(f =q)"` | **SIGSEGV** at run time |
| `int t=$(total =q)` | **ICE: undefined arrlen: A** |

The capture paths additionally reported a spurious SB5050 on `q`. Per the
repo's own standing note that the dead-variable gate runs last and masks the
real diagnostic, that is exactly what happened here — it cost two rounds of
misreading before an independent read of `q` exposed the `len A=0` underneath.
Part F's "`=foo` **is** a read when the callee reads the slot" already settled
that SB5050 must not fire; validate's `collect_slot_arg_reads` simply never
descended into `$( )`.

`check_call_args_in` had the same gap, and it was the worse one: Part C
(scalars first), the slot-count limit and the slot-operand rules were **not
checked at all** inside a capture, so `str t=$(f =q "x")` compiled silently.
Both now descend with the existing `for_each_cmdsub_shallow` helper.

Because a capture is materialised after lowering, the slot names have to ride
on the value: `SsaValue::FuncCall` and `IntFuncCall` gained a `colls` field.
`try_int_tail_call` refuses to take the tail-call path when `colls` is
non-empty — that loop rebinds `args` only, so it would drop the slots; failing
closed to the general path is correct and costs nothing measurable.

Specs `collparam.sh` 100.38-100.45. journal 443.

## Addendum (2026-10-31) — the caught attest-all retagged the slot's name, not the slot

Part F's "Substitution has to cover every write path" made both attest-all
sites resolve a slot through `resolve_args_name`. The caught attest-all has a
further write: its pass branch retags the collection clean. The caught forms
are `if attest v in A [[ … ]] { … }` and `attest v in A [[ … ]] || cmd`. That
retag used the literal name `A` (batch-531 B92). So the check read the
caller's collection, while the retag defined a version of `A` that no code
reads. There were two symptoms:

- natively, codegen found no slot for that version and ICEd (`no codegen slot
  for A_3`, types.rs:72); C ran;
- when the body read the slot afterwards, the SSA taint backstop still saw the
  caller's collection as tainted. It rejected the read, in both backends, in a
  program the validator had accepted.

The retag is now one helper. The statement form and both caught forms use it,
and it resolves the slot like every other path. This is Part F's own rule
("Any new path that names a collection must call `resolve_args_name`"),
applied to a path the rule missed.

Specs:

- collparam.sh 100.28 already ran the statement form, and it now runs through
  the helper. Without the resolution, it fails natively. Its body attests a
  slot it received, which DD#127 makes SB5088; when W1 flips it, W1 adds a
  spec in which a body fills a slot and attests it, so the statement form
  stays pinned.
- taint.sh atslot.7 (the `if` form, with a later read) and atslot.8 (the `||`
  form) pin the caught forms. Each body fills its own slot and attests it,
  which DD#127 keeps legal.

Journal 553.
