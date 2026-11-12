title: DD#92 — a bare `@args` splat to an external command implies min arity ≥1
status: accepted

1. A bare `@args` argument to an external command in a function requires
   at least one scalar argument at every call site. That static arity
   obligation replaces the ordinary external-splat empty-disposition.
2. This exemption belongs only to the function's `args`; named collections
   retain DD#69's disposition requirement. Iterating `@args` alone does
   not impose an arity floor.
3. Dispositioned `args` splats do not impose that floor: `:-` supplies
   one default argument on empty, `:?` aborts on empty, and `:~` elides.
4. `args` is the function's immutable, clean argument collection.
   Attest-all cannot target `args`. Passing scalar arguments reads them
   at the call site and therefore requires legal clean or explicitly
   trusted values.

---

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

A bare `@args` splat (operator `arrvals`, name `args`; unquoted ~~or a single-part
`"${args[@]}"`~~) as an argument to an **external** command (post-DD#23 resolution,
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
  `@{args:-}` accepts 0 args (empty → ~~zero args,~~ one empty argument, no floor); `@{args:?}` accepts
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

## Addendum (2026-09-05) — `args` is immutable and always clean (SB5052)

DG: "could I ever pass a tainted arg to a function — I don't think so; passing
the arg counts as a read of the value." Correct, and confirmed: a value passed to
a function is read at the call site, so a **tainted** one is SB5020 unless blessed
first — ~~even a tainted *default* (`${1:-$(getenv HOME)}`) is caught~~. So the args a
function **receives** are unconditionally clean; the SSA backstop already relies
on this (it seeds `$1..$9`/`$@`/`$*` as Clean "by DD#24", `ssa_taint.rs`).

To make "args is always clean" a *true whole-lifetime invariant* (not just "as
received"), DG chose (2026-09-05) to make the `args` array **immutable**: every
write to it is rejected with **SB5052** —

- assignment / subscript-assign: `args=…`, `args[i]=…`
- bulk fill: ~~`read -a args`~~, `argv … args`, ~~`mapfile args`~~
- declaration: `array args=…` / `map args=…`
- ~~**`bless … in args`**~~ **`attest … in args`** — now a clean SB5052 rejection (it was a codegen ICE,
  `bug-bless-all-args-array-ice`), and meaningless anyway: `args` can't be
  tainted, so there is nothing to bless.

Use a differently-named local array. Reading `args` (`@args`, `${args[i]}`, `$1`,
`$#`, …) is of course unaffected. Impl: `validate.rs`
`check_args_immutable_stmt` (+ `command_writes_args` / `argv_target_name` /
`mapfile_target_name` / `bless_array_target`). Specs `optional_args.sh`
84.25–84.30. SB5052 is a new code.
