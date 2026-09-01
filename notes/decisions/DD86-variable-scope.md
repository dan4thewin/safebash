title: DD#86 — Variable scope: no `local`, no shadowing, use-before-and-after
status: accepted

Settled with DG (2026-09-01). Replaces bash's `local`/dynamic-scope model with a
**structural** one. Supersedes the *lifetime* axis of DD#35, abandons DD#18
(`local fn`), and pairs the DD#62 read-side typo check with a new write-side one.

## The problem

bash's default is dynamic scope: a bare `x=v` in a function writes a *global*,
and you must remember `local x` to opt out. Forgetting `local` silently clobbers
a global; using `local` creates a **shadow** (a local hiding a same-named
global) that a reviewer can't trace. Both are footguns.

## The model

- **Scope is positional, not keyworded.**
  - A variable **declared at top level** is a **global**.
  - A name **first assigned inside a function** that is *not* a top-level global
    is a **function-local** (automatic — no keyword).
- **Globals are read *and* written with no friction** from any function. The
  method-like pattern (a set of functions manipulating shared top-level state)
  is first-class — reading and writing a global look identical to a local.
- **No shadowing.** A name is a global *xor* a local — never a local hiding a
  global. Consequently a *typed re-declaration* inside a function of a name that
  is already a top-level global is an **error** (redeclaration); a *bare*
  assignment to that name simply **writes** the global.
- **`local` is removed** (keyword + the save/restore machinery). Lifetime is now
  positional, so the DD#35 *lifetime* axis is retired.
- **`local fn` (DD#18) is abandoned** — it was the only other user of `local`,
  and "designed, not built."

## Declaring globals — the `str` keyword

Globals must be introduced at top level, so a way to declare each type
*without* an initializer is needed. `int n` already declares an int
(meaningful-zero `0`, DD#62). Add the missing scalar:

- **`str name`** — declares a **string** variable, meaningful-zero `""`
  (parallel to `int`). `str name = "x"` also allowed.

Full type-keyword set is now `str` (string) / `int` / `array` / `map`, with
`const` orthogonal (mutability). A bare top-level `x = v` still declares a global
(inferred string), same as today; `str` is for the declare-without-assign case
(and for making a string declaration explicit).

## Use-before-**and**-after: reject unused variables

DD#62 already rejects a **read** with no reaching assignment (SB5031, "assigned
nowhere"). DD#86 adds the dual: **a variable that is assigned but never read is
an error** (dead variable). Every variable name must therefore appear in *both*
a write and a read.

This is the safety net for "no shadowing + frictionless writes": a misspelled
target is caught from whichever side it is stranded on, with **no warning** —
warnings signal ambiguity, and there is none here, so it is a hard error:

```
str count
fn inc { coutn = count + 1 }   # `coutn` local, assigned, never read → ERROR
fn inc { count = coutn + 1 }   # `coutn` read, never assigned         → SB5031
```

Because globals must be top-level-declared and there is no dynamic scope
(a callee cannot see a caller's local), a misspelled name is always a
per-function local — so it is caught by the unread-write rule if the typo is on
the write, or the unassigned-read rule (SB5031) if it is on the read (a local
never crosses to another function). The only uncatchable case is a typo that
lands on *another* declared global — fundamentally undetectable, and rare.

`const` is **not** exempt: an unread `const` is dead → error.

## Explicitly rejected alternatives

- **Warnings** for "function writes a global it never reads" — rejected. No
  compiler warnings at all; ambiguity is a hard error.
- **Loop-variable auto-scoping** — rejected as YAGNI. A `for` / C-style loop
  variable is an ordinary variable (may persist after the loop; use a `_i`
  convention if a same-named global is a concern).
- **Read-only globals from functions** (an earlier "Model A") — rejected;
  globals are freely writable.

## Consequences / migration

- **Converter**: bash `local x` → drop the keyword (the variable is
  auto-local). bash functions that write globals keep working (writes stay
  frictionless). A bash local that *shadowed* a global must be renamed
  (no-shadowing) — the converter flags it for review.
- **DD#35** amended: lifetime axis retired; `str` added to the type axis.
- **DD#18** abandoned.
- Spec suites using `local` are rewritten to the keyword-free form.

## Implementation phases

1. `str` keyword (additive). **— shipped (journal 223).**
2. Drop `local` + make scope structural (top-level global vs function-local),
   frictionless global R/W. **— shipped (journal 226); `local fn` (DD#18)
   retired in the same phase since it shared all the machinery. Surfaced +
   fixed a latent bug: a function-local `const` tripped SB4044 because the
   body is lowered twice and `readonly_vars` leaked (isolated via
   `enter/exit_fn_body_scope`).**
3. No-shadowing / redeclaration error.
4. Dead-variable rejection (new validate pass, dual of DD#62).
5. Retire `local fn`; converter; specs; GUIDE/SUBSET. **— `local fn` done in
   Phase 2; remaining: converter `local`→drop, GUIDE/SUBSET, DD#35 amend.**
