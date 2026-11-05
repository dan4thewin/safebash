title: DD#81 — function-call discipline: one name one meaning, arity from `args`
status: accepted

Settled with DG (2026-08-25). Two coupled decisions about user-function calls:
**(A)** a bareword name resolves *consistently* — command/function shadowing is
rejected; **(B)** a function's arity is inferred from its `args` usage and
checked at every call site.

## Background — the thread that produced this

Investigating a forward-reference ICE surfaced three entangled problems, all one
root cause — the compiler had **two disagreeing "is this a function?" gates**:

- the plain-call path keys off `function_bodies` (defined-so-far, execution
  order) — which correctly models bash's command/function name resolution;
- the `$(f)` capture path keys off `known_functions` (collected *upfront*,
  including not-yet-defined names) — which does **not**.

That mismatch caused: (1) `$( )` command shadowing to break (`x=$(cat …); fn cat
{…}; y=$(cat …)` printed the function *both* times — bash: external then
function); (2) a forward `$(f)` to ICE (it fell into a half-baked `func_constants`
path instead of resolving external); (3) arity checking to have holes (it only
runs where the body is available at inline time).

DG's key observation: the execution-order behaviour of the *plain* path is not
an inlining accident — it is exactly what makes the common bash idiom "call the
real `dd`, then define `dd()` to wrap it" work, and a whole-program/2-pass model
(C/Go style) would break it. So the semantics are right; the defect is that the
`$( )` path diverges. But DG also judged the *dynamic* half of shadowing (a name
silently meaning two different things at two points) a **footgun** — to be
rejected, not faithfully emulated.

## Part A — one name, one meaning

### Rule

A bareword command name resolves, at each point it executes, to a **user
function** if one by that name is in scope (defined earlier in execution order),
otherwise to an **external command**. It is a compile error (**SB5041**) for a
single name to resolve to a function at one point and an external at another —
equivalently, for a `fn NAME` definition to be preceded, in execution order, by
a use of `NAME` that resolved to an external.

Detection is a simple forward pass: record each name resolved as an external; on
`fn NAME`, if `NAME` is already in that set, reject. (Inlined bodies resolve at
inline time, i.e. at their call's execution point, so an external use inside a
body that runs before the definition is caught too.)

### Rejected vs kept

| rejected (SB5041) | kept |
|---|---|
| dynamic shadowing: `cat …; fn cat {…}; cat …` | **static override**: `fn cat {…}` used throughout (call the real one as `/bin/cat`) |
| forward function call: `echo $(f); fn f {…}` (f resolves external, then is defined) | **helper below caller**: `fn a { b }; fn b {…}; a` — `b` is only ever *called* (when `a` runs, `b` is defined) and never resolves external |
| forward plain call: `f x; fn f {…}` | **explicit external**: `/bin/cat` always reaches the real command |

### Rationale

One name / two meanings by textual position is a readability and audit hazard,
and it is at odds with sash's compile-time command resolution (DD#23) and its
safe-subset ethos (reject the ambiguous rather than silently pick a meaning).
Rejection is a *clean* divergence from bash — a program that won't compile, not
one that miscompiles — and the *useful* half of shadowing (wholesale override,
explicit external) survives.

### Consequences

- **The forward-reference policy is subsumed** — a forward call is a
  use-before-definition, which is a collision. No separate forward-ref rule is
  needed.
- **`func_constants` / `lower_func_call_value` / `fold_function_outputs` become
  provably dead → removed.** That machinery existed only to (badly) handle a
  name used before its function definition; forbid that and it evaporates. This
  is the "dead code" question that started the thread — it is dead *by
  correctness*, once Part A holds.
- **The `$(f)` capture path unifies onto the plain-call `function_bodies`
  gate** (function-if-defined-so-far, else external command substitution). This
  restores bash-faithful `$( )` shadowing and removes the forward-`$(f)` ICE.

## Part B — function arity from `args`

### `args` recap (DD#75 §5)

`args` is this function's own argument array, **0-indexed**. Per DD#75 §5 the
bash positional sigils are pure **sugar** for the ordinary collection surface —
all referring to the *same* per-function `args`:

| sugar | canonical | operator |
|---|---|---|
| `$1`..`$n` | `${args[0]}`..`${args[n-1]}` | subscript (`$1` ≡ `${args[0]}`) |
| `$@` | *(removed)* → `@args` | splat |
| `$#` | `$(len args)` | length (`${#args}` is operator sugar) |
| `$*` | *(removed)* → `$(join args)` | — |

`$0` is separate — the program name (DD#60), not part of `args`. So `$@` is
**this function's own args** (≡ `@args`), *not* the caller's.

Two current-state notes: indexed access is unchecked today (`${args[2]}` with
one arg yields empty); and — **`$@`/`$*` are dropped** (DG 2026-08-25;
extended to `$*` and made universal 2026-08-26): a `$`-sigil that splats/joins
contradicts DD#75 §4, so `$@`/`$*` (and the braced `${@}`/`${*}`, and inside
`$(( ))`) are rejected **everywhere** (**SB5045** "sash has no `$@`/`$*`") — in
a function → `@args` (splat) / `$(join args)` (joined); at top level →
`$(argv 1)` / `$(arg N)` (DD#46). `$1`/`$#` stay — genuinely scalar. This DD
makes the surface a checked signature over `@args`/`${args[i]}`/`$N`.

### Signature inference

- **Literal index** `${args[k]}` / `$K` → contributes index `k`. The signature
  is the contiguous set `{0..max}`; a gap is a **hole** (**SB5044**). A function
  with only literal indices and no whole-args access has **fixed arity =
  max+1**.
- **Whole-args access** — `@args`, `$(join args)` / `$(keys args)`, or a
  **dynamic** index `${args[$i]}` — makes the function **variadic** (any
  count; floor = highest literal index + 1 if any is also present).
  *Amended 2026-09-28 (see the addendum): `$(len args)` / `$#` are **not**
  whole-args access — they read the count, not an argument.*

### Checking — at *every* call site

Enabled by Part A (every call resolves to an already-defined function, so its
signature is always known):

- **fixed arity N** → the call must pass **exactly N** (too few → **SB5042**;
  too many → **SB5043**).
- **variadic, floor M** → the call must pass **≥ M** (fewer → SB5042).
- **a splat or a `:~` elide** → legal only into a variadic callee, and only
  where every position below the floor is still fixed — in practice,
  **required arguments go before the splat** (otherwise **SB5087**). See the
  addendum of 2026-09-28 below; the mechanism is DD#84.

### Edge rules

- `@args` **and** a literal index → variadic **with a floor** (the "first arg
  special, rest variadic" pattern).
- `$(len args)` / `$#` (the arg count) → **nothing**; the count is not an
  access (amended 2026-09-28 — was "→ variadic").
- dynamic index `${args[$i]}` → variadic (a runtime index can't be a static
  slot).
- `shift` → variadic (moot today; `shift` is unimplemented, SB5011).

### Holes — decided (a)

**A fixed-arity signature must read the full contiguous set `{0..max}`.** A gap
is a compile error (**SB5044**): `${args[1]}`-only is a hole at index 0, and
`${args[0]}`+`${args[2]}` is a hole at index 1. A genuinely-unused parameter
must be consumed via `@args` (variadic) or the function reordered so its
parameters are `0..max` with no gap. (Rejected alternative: allowing a leading
unused slot — chosen against for clarity; DG 2026-08-25.)

## SB codes

- **SB5041** — command/function name collision (Part A).
- **SB5042** — function called with too few arguments (absorbs today's un-coded
  `function 'f' reads $N but called with only M` diagnostic).
- **SB5043** — function called with too many arguments (fixed-arity function).
- **SB5044** — function signature has a hole (non-contiguous `args` indices).
- **SB5087** — a call whose argument count is only known at run time (a splat
  or a `:~` elide) into a callee that cannot take one: a fixed-arity function,
  or a variadic one whose fixed positions the call does not pin down
  (2026-09-28).

## Implementation plan (staged, each a green commit)

1. **Unify the `$(f)` gate** onto `function_bodies` + external-command fallback
   (bug fix on its own: restores `$( )` shadowing, kills the forward-`$(f)`
   ICE). Spec the shadowing idiom (plain and `$( )`).
2. **Add the SB5041 collision check** (reject `fn NAME` after an external use of
   `NAME`).
3. **Remove the dead `func_constants` machinery** (`fold_function_outputs`,
   `lower_func_call_value`, the `FuncCall` value arm, the threading).
4. **Signature inference + uniform arity check** across plain / pipeline / `$( )`
   paths, including `${args[i]}` bounds → arity and the hole check.
5. **Clean the runtime message**: a genuinely-unresolved external →
   `command not found` / 127 (not `bug: bare command passed to sash_exec`).
6. **Fix `$(f arg)`**: a defined function called with args in `$( )` currently
   drops the args and ICEs (`undefined array`); pass and arity-check them.

## Related bugs surfaced in this thread

- `$(f arg)` drops args + ICEs (`types.rs` `undefined array`) — step 6.
- `bug: bare command passed to sash_exec` for an unresolved bareword — step 5.
- *(Not a bug:* `$@` inside a function reading empty is DD#60 — that is the
  caller's args, which are empty at top level.)

## Relates

DD#23 (compile-time command resolution), DD#60 (caller's `$@`/`$*`), DD#75
(uniform collections / the `args` array), DD#84 (optional trailing args and
the `:~` elide — the minimum-token table the 2026-09-28 addendum below rests
on), DD#69 (splat dispositions — `@{a:-W}`/`@{a:?W}` are what let a splat fill
a fixed argument position), DD#18 (local functions — the natural
home for an *explicit* unshadow mechanism, e.g. `unset -f` / scoped functions,
should controlled dynamic behaviour ever be wanted back).

## Implemented

- **Part A** (d73c69e) — SB5041 shadowing/forward-ref rejection; `$(f)` gate
  unified onto the plain-call `function_bodies` gate. + func_constants removal
  (56e4c17), `$(f arg)` capture args (50474e9).
- **Part B** — `transform/functions.rs::analyze_arity` infers `Arity`
  (`Fixed(n)` / `Variadic{floor}`) by walking the body's `args` reads
  (`$N`/`${args[k]}` literal → fixed index; `@args`/`$(join args)`/
  `$(keys args)`/`$@`/dynamic index → variadic; **including inside `$(( ))`** —
  arith is scanned via `sig_scan_arith`, else a positional in a condition is
  missed). Checked on
  all three call paths (plain, pipeline, `$( )` capture): fixed → exactly N
  (**SB5042** too-few / **SB5043** too-many); variadic → ≥ floor (SB5042). A
  fixed signature must read `{0..max}` contiguously — a gap is **SB5044**
  (rule (a)). *(`$(len args)`/`$#` were in the variadic set until the
  2026-09-28 addendum below.)*
- **The no-op idiom** (DG 2026-08-25): to acknowledge an argument a fixed-arity
  function *takes* but does not otherwise use, reference it in a no-op — `fn f
  { : $1; echo $2; }` (reads index 0, no hole). To take a *variable* number (or
  reach a high index without the lower ones), mark the function variadic with
  `@args` — `fn f { : @args; echo ${args[10]}; }`. specs functions.sh 81.9–81.13.

## Addendum (2026-09-28) — counting the arguments is not accessing them

**DG:** *"`$(len args)` is how a function would loop over variadic args; but
accessing an arg via a loop or branch is what would make it variadic. Without
the loop/branch the len is vacuous."*

The original Part B put `$(len args)` / `$#` in the whole-args set, so
**observing the argument count erased the arity ceiling**:

```sash
fn f { str a=${args[0]}; str b=${args[1]}; echo "$(len args) [$a][$b]" }
f p d x y     # → "4 [p][d]"   — accepted, two arguments silently discarded
```

Delete the `$(len args)` and the same call is SB5043. One `len` anywhere in
the body turned `Fixed{2,2}` into "any count".

### The rule

Arity is inferred from how a body **accesses** its arguments, never from
whether it counts them:

| in the body | contributes |
|---|---|
| `${args[k]}` / `$N` literal | fixed index `k` (required) |
| `${args[k]:-}` / `${args[k]:~}` | fixed index `k` (optional, DD#84) |
| `${args[$i]}` dynamic index | **variadic** |
| `@args` splat | **variadic** |
| `$(join args)` / `$(keys args)` | **variadic** (both read every element) |
| `$(len args)` / `$#` | **nothing** |

`len` earns its place in a variadic function as the *loop bound*, and the
access inside that loop is already a dynamic index — so the canonical form
stays variadic without `len` contributing anything:

```sash
fn f {
  int n=$(len args)
  int i=0
  while (( i < n )); do echo "[${args[$i]}]"; i=$(( i + 1 )); done
}
f a b c        # → [a] [b] [c]      (variadic via ${args[$i]})
```

`for x in @args` and `$(join args)` likewise keep working on their own
access. **No variadic idiom depends on the old rule.**

### What this rejects, and the escape hatch

A body that counts but never accesses is now fixed-arity at whatever its
literal indices say — `{0,0}` if there are none:

```sash
fn n { echo "$#" }
n a b
→ error [SB5043] function 'n' takes 0 arguments but was called with 2
  — use @args to accept a variable number
```

That is DD#81's **no-op idiom**, unchanged since 2026-08-25, and SB5043 was
already worded to point at it. A function that genuinely wants its own count
declares that it takes any number:

```sash
fn n { : @args; echo "$#" }
n a b          # → 2
```

This is a **breaking change** — a bare `$#`/`len args` counter now needs
`: @args` — and it is fail-closed: it rejects programs, never accepts new
ones. Migration is one token per function.

### Why it is the right default

sash checks arity at compile time (SB5042/SB5043), so a hand-written runtime
`if (( $# != 2 ))` guard is redundant with a guarantee the compiler already
makes. Under the old rule *writing that guard silently removed the
guarantee it was imitating*. Making the count contribute nothing means the
only way to opt out of the compile-time check is to say so — `: @args` —
rather than to mention `$#` in passing.

### Cost, measured

17 specs asserted the old behaviour, every one of them a count-without-access
body: types.sh 75.18/75.23/75.89/75.94/75.95/75.97/75.98, functions.sh
sp.1/sp.8, optional_args.sh 84.50/84.51, argv.sh av.h1/av.h3, arithmetic.sh
31.24, control_flow.sh cf.f13, subscript.sh 80.32, coverage.sh cov.241. All
were migrated by adding `: @args`. No spec exercising a real variadic access
moved.

### Not changed — "branch"

DG's phrasing was "a loop **or branch**". A *conditionally* read literal index
(`if …; then str b=${args[1]}; fi`) still contributes a required fixed index
rather than making the function variadic, because DD#84 already gives that
shape a tighter spelling — `${args[1]:-}` → `Fixed{1,2}`, which states the
optionality instead of discarding the ceiling. Left as-is pending DG.

## Addendum (2026-09-28) — required arguments go before the splat

Part B above checks a call by **counting** its arguments. That works only when
the count is knowable, and two argument forms are not: an array splat (`@a`,
`@{a:…}`) and a `:~` elide, each of which is *one* thing written at the call
site and 0..n arguments at run time.

DG settled what they may be passed to:

> an elision, given it could remove an argument, can only work for a variadic
> context and not a fixed one. similarly, a splat should only work for
> variadic given it could be empty

and then, on how that interacts with a floor:

> in my mental model, a function can have both a fixed portion and a variadic
> one with the fixed met by discrete arguments as though there were no
> variadic component. I grant that `@{e:-fallback}` can be guaranteed to
> provide 1 argument.

### The rule, as an author sees it

**Write the required arguments first, then the splat.**

```sash
fn f { : @args; str host=${args[0]}; str port=${args[1]}; … }   # floor 2

f "$h" "$p" @rest      # correct
f @rest "$h" "$p"      # SB5087
```

This is the shape almost everyone writes by reflex; the rule mostly matters
because the *other* order used to compile.

### Why — it is not a count, it is a position

A function's fixed portion is met by discrete arguments as though the variadic
part were not there. So the question at each of positions `0..floor-1` is not
"are there enough arguments" but "**is the thing the author wrote at this
position the thing that will be here at run time**".

A splat breaks that for everything behind it. `@rest` with two elements pushes
`"$h"` to position 2; with none, it leaves it at 0. The compiler cannot know
which, so nothing after a splat can satisfy a fixed read.

Counting hides this, and the failure is silent rather than loud:

```sash
fn f { : @args; echo "first=[${args[0]}]" }
array e=(q r)
f @e x
```

The floor is 1 and the call passes at least one argument, so a count-based
check accepts it. At run time `${args[0]}` is `x` when `e` is empty and `q`
when it is not — a named parameter silently taking a *different variable's*
value depending on an array's length, with nothing reported at compile time.
That is the bug this rule exists to prevent, and it is worse than the arity
error it superficially resembles.

### What still counts as pinning a position down

Walking the argument list left to right:

- a plain argument fixes exactly its own position;
- a splat or elide fixes the positions its **minimum** guarantees (DD#84's
  table: `@{a:-W}` and `@{a:?W}` guarantee one; bare `@a` and `@{a:~}`
  guarantee none), and **ends the walk** — no position after it is knowable.

So DG's grant holds: `@{e:-fallback}` guarantees an element, so it can fill
position 0 of a floor-1 callee. It cannot fill position 1 of a floor-2 one,
because it may also deliver three elements and move whatever follows.

### The escape hatches

- put the required arguments before the splat (the ordinary fix);
- guarantee the element with DD#69's `@{a:-WORD}` when the splat genuinely is
  the required argument;
- drop the floor — read the index through a DD#84 disposition
  (`${args[0]:-}`), which makes it optional and the floor 0, at which point
  order is irrelevant because nothing is read positionally.

Impl `transform/cmd_handlers.rs::call_arg_bounds`. Specs `optional_args.sh`
84.53-84.72. journal 443.

## Argument materialization repair (2026-10-18)

An argument's nested calls execute before the callee's `args`, collection
slots or inline context replace the caller's. Plain calls, function captures
and function pipeline stages now use the same argument lifter as integer
calls. The pipeline path retains its existing child boundary: argument-side
changes stay in that stage. Call redirects are taken out of pending state
before lifting, and opened afterwards, so an argument capture cannot steal
the outer call's redirect.

This repairs unevaluated function values reaching codegen, false recursion
for finite `f $(f leaf)` nesting, and caller positional/capture boundaries;
it does not introduce multiple signatures or a new pipeline evaluation
boundary. `functions.sh` `tmpcap.4`, `tmpcap.13`-`tmpcap.18`; journal 526.
Tail producers use the same materializer and preserve splats (DD#27).

## Cleanup calls and definition placement

Cleanup bodies may call program-level helpers under Part A's call
discipline; they are not additional definition roots. SB5071 rejects a
definition inside EXIT or ERR uniformly.
The former F3 exception was a misreading of DD86, corrected in journal 566
and DD56. Its definition-order specs `56.97` and `56.98` are retired.

The independent SB5041 wording defect remains for valid program-level
definitions preceded by arithmetic uses: the message says "used as a
command" (`bug-sb5041-text-for-arithmetic-uses`).

## Addendum (2026-10-29) — one signature, inferred once, checked at every call

Batch-531 F5 unit B (journal 543).

- **Every `-> int` call is checked.** Part B's three checked call paths did
  not include an `-> int` function's: `(f …)` and `$(( (f …) ))`,
  `int n=$(f …)`, `(( (f) > 0 ))`. A missing argument read as empty and an
  extra one was ignored (B56). `inline_int_func` now raises SB5042/SB5043
  like the plain path, and so does `lower_int_tail_call`: since batch-531
  F13 an int function's self tail call lowers as a loop instead of being
  inlined (`81.55`-`81.61`).
- **The signature comes from the shared walk.** `analyze_arity` runs
  `ir::walk_scoped_stmts` and `ir::walk_own_exprs`, the traversal the other
  passes use, and stays in the function's own scope. The private copy it
  replaces (Part B's `sig_scan_arith` belonged to it; the arithmetic half is
  now `sig_record_arith_names`) ended in `_ => {}`. It missed a positional
  read in an assignment index, a dispatch pattern, a `${…}` subscript under
  a default, a redirect target, a map literal and `$(<$1)`, so a function
  reading `$1` only there drew a false SB5043 at every call (B53,
  `81.47`-`81.53`).
- **`$(- join args)` is whole-args access**, like `$(join args)` (B49,
  `81.54`). `-` changes how a status is checked (DD#55), not what runs, and
  `join` is an intrinsic, so this is not `-` on a user function
  (`q-suppression-on-user-functions`).
- **`$(keys args)` is not whole-args access.** Part B and the table above
  list it, but `keys` takes a map, so it is SB4030 (`75.100`). The scan no
  longer looks for it.
