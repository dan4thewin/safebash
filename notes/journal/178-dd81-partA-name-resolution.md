# 178 — DD#81 Part A: one name, one meaning (reject shadowing)

First implementation stage of DD#81. The `$( )` capture path and the plain-call
path disagreed on "is this a function?" — the plain path used `function_bodies`
(defined-so-far, execution order, bash-faithful), the `$( )` path used
`known_functions` (collected upfront). That mismatch broke `$( )` command
shadowing and turned a forward `$(f)` into a codegen ICE.

## The change

- **Unify the gate**: `handle_command_substitution` now resolves `$(f)` to a
  function only if `f` is in `function_bodies` (defined so far), else it falls
  through to an external command substitution — exactly the plain-call gate.
- **SB5041 collision**: a new `externally_used` set (interior-mutable, like
  `external_deps`) records every bareword resolved as an *external* command. On
  `fn NAME`, if `NAME` is already there, it's a command/function shadowing
  collision → SB5041. This subsumes the forward-reference case (a forward call
  resolves external, so the later `fn` collides).

## The template-block trap

First cut wrongly rejected `fn a { b }; fn b {}; a`. Cause: `lower_function_stmt`
eagerly lowers a standalone `fn_{name}` **template** block at definition time
(a dead template — every real call inlines). That template's `lower_stmts`
resolved `b` before `fn b` existed → a false external record. Fix: a
`lowering_fn_template` flag gates the `externally_used` recording off during
template lowering (those resolutions are not real execution order). Real records
come only from top-level lowering and inline-at-call lowering.

## Behaviour (verified)

Rejected (SB5041): `cat …; fn cat {…}; cat …` (dynamic shadow); `echo $(f); fn f`
(forward `$(f)` — was an ICE); `f x; fn f` (forward plain — was a runtime
`bug:`); `$( )` shadow. Kept: helper-below-caller (`fn a { b }; fn b {}; a`),
`$(b)` variant, acyclic `a→b→c`, static override (`fn cat {…}; cat`), normal
`$(f)`.

## Specs / results

`spec/functions.sh` 81.1–81.6 (81.4/81.5 are runtime → `native,c`; janet
resolves an in-body plain call as an external exec, a pre-existing backend gap).
Full `SPEC_MODES="native c janet"` sweep vs base `820f0cd`: **0 regressions**,
native gate 0 failing, 16 new passing.

## Next (DD#81 remaining)

With forward `$(f)` rejected, the `func_constants` / `lower_func_call_value` /
`fold_function_outputs` machinery is now provably dead → remove it (next stage).
Then `$(f arg)` arg passing, the uniform arity check (Part B), and the
`bug: bare command` → `command not found` message cleanup.
