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
| `$@` | `@args` | splat |
| `$#` | `$(len args)` | length (`${#args}` is operator sugar) |
| `$*` | *(removed)* → `$(join args)` | — |

`$0` is separate — the program name (DD#60), not part of `args`. So `$@` is
**this function's own args** (≡ `@args`), *not* the caller's.

Two current-state notes: indexed access is unchecked today (`${args[2]}` with
one arg yields empty); and — a known **DD#75 §5 migration gap** — the `$@` sugar
does not yet desugar to `@args` (`$@` reads empty inside a function while `@args`
is correct; a stale pre-DD#75 path remains). This DD makes the surface a checked
signature and assumes/repairs the `$@ ≡ @args` equivalence.

### Signature inference

- **Literal index** `${args[k]}` / `$K` → contributes index `k`. The signature
  is the contiguous set `{0..max}`; a gap is a **hole** (**SB5044**). A function
  with only literal indices and no whole-args access has **fixed arity =
  max+1**.
- **Whole-args access** — `@args` (≡ `$@`), `$(len args)` (the count; ≡ sugar
  `${#args}` / `$#`), or a **dynamic** index `${args[$i]}` — makes the function
  **variadic** (any count; floor = highest literal index + 1 if any is also
  present).

### Checking — at *every* call site

Enabled by Part A (every call resolves to an already-defined function, so its
signature is always known):

- **fixed arity N** → the call must pass **exactly N** (too few → **SB5042**;
  too many → **SB5043**).
- **variadic, floor M** → the call must pass **≥ M** (fewer → SB5042).

### Edge rules

- `@args` **and** a literal index → variadic **with a floor** (the "first arg
  special, rest variadic" pattern).
- `$(len args)` (the arg count; sugar `${#args}` / `$#`) → variadic.
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
(uniform collections / the `args` array), DD#18 (local functions — the natural
home for an *explicit* unshadow mechanism, e.g. `unset -f` / scoped functions,
should controlled dynamic behaviour ever be wanted back).
