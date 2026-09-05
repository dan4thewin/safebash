title: DD#92 — a bare `@args` splat to an external command implies min arity ≥1
status: accepted

Settled with DG (2026-09-05). A bare `@args` splat passed to an **external
command** inside a function is **valid without a disposition** — it is exempt
from SB5032 (DD#69). Instead, it makes the function require **≥1 argument**: the
arity analysis records a required index 0 (as if the body also read `$1`), so the
splat is provably non-empty and a 0-arg call is rejected at compile time
(SB5042).

## Motivation

DG: "we have arity checking for functions. we should be able to forego the SB5032
or `@args:?` modifier for args in a command within a function when the arity check
knows there's always at least one argument. … treat `fn f { tool @args }` as if
it were `fn f { : $1; tool @args }`."

DD#69 requires a bare `@a` splat into an external command to carry an
empty-disposition (`@{a:-WORD}` / `@{a:?WORD}` — SB5032), because an empty array
would run the command with too few arguments. For the function-args array
`@args` that is needlessly heavy: the splatting function almost always *means*
"my arguments, of which there is at least one". The arity model (DD#81/#84)
already enforces a minimum arg count at call sites, so the guarantee can be
expressed structurally instead of with a runtime `:?` abort.

## The rule

A bare `@args` splat (operator `arrvals`, name `args`; unquoted or a single-part
`"${args[@]}"`) as an argument to an **external** command (post-DD#23 resolution,
an absolute-path head):

- **Waives SB5032** — it is valid without a disposition (validate,
  `check_external_splat`).
- **Records a required index 0** in the arity signature (transform,
  `analyze_arity` / `sig_scan_stmt`) — exactly as a bare `$1` would. The function
  becomes variadic with `floor ≥ 1`.

Consequences, all from the existing arity machinery:

- A caller passing **0 args** is **SB5042** (too few) — so the external never
  runs with an empty splat. Soundness comes from this, not from a runtime check.
- The function is still variadic (any count ≥ 1).

## Scope and non-goals

- **`@args` only.** A general `@a` keeps SB5032 — DD#69 deliberately has no
  static-non-empty exemption, and only `args` carries an arity guarantee.
  `array a=(x); tool @a` is still SB5032.
- **External only.** `for x in @args` and builtin `@args` handle emptiness
  themselves (0 iterations / their own rules), so they impose **no** floor — a
  for-in-only function stays 0-arg-callable. Matches SB5032's external-only scope.
- **Top level is unaffected.** `@args` does not exist at top level (SB5024), so
  the rule only ever applies inside a function.
- **The disposition forms still work and mean something different:**
  `@{args:-}` accepts 0 args (empty → zero args, no floor); `@{args:?}` accepts
  the call and aborts at *runtime* on empty. Use those when the function should
  accept 0 args; bare `@args` when it should not.

## Why sound

`floor ≥ 1` is enforced at every call site (SB5042 too-few-args, DD#84). A
function is only reachable through calls that pass ≥ floor args, so inside the
body `args` has ≥1 element and the external splat is non-empty. The validate
waiver and the arity floor key on the **same** predicate (a bare `@args` splat in
an external command, both seeing the DD#23-resolved absolute head), so they
cannot drift.

## Implementation

- `validate.rs` `check_external_splat` + `bare_arrvals_splat_name`: skip SB5032
  when the bare splat's name is `args`.
- `transform/functions.rs` `sig_scan_stmt` + `is_external_head` /
  `is_bare_args_splat`: a bare `@args` in an external command's args does
  `sig.required.insert(0)`.

Specs `glob_splat.sh` (gs.31+). No new SB code (reuses SB5042 / SB5032 / SB5024).

## See also

- DD#69 — external-splat empty-disposition (SB5032); the static-non-empty
  non-exemption this preserves.
- DD#81 / DD#84 — the arity model (required/optional/variadic, floor, SB5042).
- DD#23 — compile-time external resolution (the absolute-path head both passes
  key on).
