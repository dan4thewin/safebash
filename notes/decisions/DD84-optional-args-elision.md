title: DD#84 — optional trailing args (`:-`/`:~`) and the scalar elision operator `:~`
status: accepted

Settled with DG (2026-08-31). Two coupled additions to the args/positional
model:

- **(A) Optional trailing arguments.** A trailing `args[i]` read *only* through a
  disposition operator (`:-` or `:~`) is **optional** — the function may be
  called with or without it. Arity becomes a range `[min, max]`.
- **(B) The scalar elision operator `:~`.** `${x:~}` / `${args[i]:~}` **elides**
  the token — it contributes *zero* tokens to a list when its value is the
  identity element — rather than expanding to an empty string. It is the scalar
  analog of the array-splat elision `@{a:~}` (an empty array → zero args,
  DD#69). (Bare `@a` on an empty array also contributes zero args, but a bare
  `@a` to an external command is SB5032 — a disposition is required.)

Amends DD#75 §5 (positional `args` model) and DD#81 Part B (arity from `args`).

## Motivation

sash does not word-split, so `${x:-}` (and `${args[i]:-}`) always expands to
**exactly one** token — an empty string, never nothing. `echo a ${x:-} b` with
empty `x` is `a "" b` (three args); `array c=(x ${y:-} z)` with empty `y` is a
**3-element** array `[x, "", z]`. There was no way to make a token *disappear*
from an argument list or array literal. Separately, `${args[i]:-}` did not make
the argument optional — arity (DD#81) still required it (SB5042), so the default
could fire only from a caller passing an explicit empty string, making `:-` on
`args` nearly inert.

`:~` is the missing "empty → disappears". It applies uniformly to scalars and to
the array-splat disposition (`@{a:~}`): `:-` always yields exactly one token
(empty string / one empty arg), `:~` yields zero. (Earlier drafts of this DD
claimed a bare `@{a:-}` already elided to zero args — that was inconsistent with
the scalar `${x:-}` and with bash `"${a[@]:-}"`; corrected 2026-09-04 so
`@{a:-}` → one empty arg and `@{a:~}` is the sole array elide. See the addendum.)

## (A) Optional trailing arguments

### Which indices are optional
An `args[i]` index is **optional** iff *every* read of it in the body goes
through a disposition operator (`:-` or `:~`). An index read even once as a bare
`${args[i]}` / `$N` is **required**.

- **Mixing is a compile error — SB5047.** The same index read both bare and via
  a disposition (`echo ${args[0]}; ls ${args[0]:~}`) is rejected: the compiler
  cannot tell whether the argument is required. (DG 2026-08-31: "mixing is
  error".)

### Arity range
For a **fixed** (non-variadic) signature:
- `min` = (highest **required** index) + 1  (0 if there are no required indices)
- `max` = (highest index read at all) + 1
- Call site must pass `min ≤ argc ≤ max` (too few → **SB5042**, too many →
  **SB5043**).

`:?` does **not** make an arg optional — it is a required-with-check read, so it
counts toward `min`.

### Structural rules (all fixed-arity)
- **Contiguity (SB5044, unchanged):** every index in `{0..max-1}` must be read
  (bare or dispositioned). A gap is an error.
- **Optional args must be trailing — SB5048.** A **required** index may not
  follow an **optional** one: required indices are `{0..min-1}`, optional
  `{min..max-1}`. `${args[0]:~}` + bare `${args[1]}` (optional-then-required) is
  rejected. (A leading optional slot would make a 1-arg call ambiguous about
  which parameter it binds.)

### Variadic is unchanged
`@args` / `$(len args)` / `${#args}` / `$#` / a dynamic `${args[$i]}` still makes
the function variadic (any count ≥ floor). A disposition on a variadic access is
allowed but confers no extra optionality (the floor already permits omission).

### Absent optional args at the call site
Functions inline per call site with the exact `arg_values` (DD#75 unique-name
model), so a call passing fewer than `max` args is known at compile time: the
callee's `args` array is materialized short. A dispositioned read of an
unmaterialized index resolves per its operator — `:-` → the default WORD, `:~` →
elision (see B).

## (B) The elision operator `:~`

### Spelling and shape
`${x:~}` / `${args[i]:~}`. **`:~` never takes a word** — `${x:~foo}` is a syntax
error (**SB4051**). If you want a value/default, that is `:-`. `@`-splat stays
collection-only and `$`/`${}` scalar-only (DG rejected an `@{scalar:-}`
spelling): `:~` is a real scalar operator alongside `:-`/`:?`.

### Semantics
In an **elidable position**, `${…:~}` contributes:
- **zero tokens** when its value is the **identity element** (empty string;
  additive-identity `0` for an int cell), or when it is an unmaterialized
  optional `args[i]`;
- **one token** (the value) otherwise.

Elision is evaluated at compile time when the value is statically known (a
literal, or an absent optional arg) and at runtime otherwise (`ls ${x:~}` with a
runtime `x`).

### Elidable positions (where `:~` is legal)
A position where a token is one of a *sequence* and dropping it is well-defined:
- an external / command argument list (`ls ${args[0]:~} rest`);
- a `sash`-varargs builtin arg list (`echo ${x:~}`);
- an array literal element (`array a=(x ${y:~} z)`).

### `:~` elsewhere is a compile error — SB5046
`:~` never yields a value, so it is meaningless where a value is required: a
scalar assignment RHS (`n=${x:~}`), a `[[ … ]]` operand, an arithmetic operand, a
redirect target, a subscript index, etc. Those are **SB5046** ("`:~` only elides
in a list position; use `:-` for a value"). This keeps the no-ambiguity rule
(the compiler always knows what a construct does) — `:~` is *only* elision, never
a silent empty.

## SB codes (new)
- **SB4051** (parse) — `:~` given a word (`${x:~foo}`); `:~` takes no operand.
- **SB5046** (validate) — `:~` in a value-required (non-elidable) position.
- **SB5047** (arity) — an `args` index read both bare and via a disposition.
- **SB5048** (arity) — a required positional follows an optional one (optional
  args must be trailing).

## Worked examples

```
fn foo { ls ${args[0]:~} report }
foo         # args[0] absent  → ls report
foo -l      # args[0] = "-l"  → ls -l report
foo ""      # args[0] = ""    → ls report   (empty = identity → elided)
# arity [0,1]: foo a b → SB5043 (too many)

fn greet { echo Hello ${args[0]:-world} }
greet       # → Hello world      (optional, default fires)
greet Sam   # → Hello Sam
# arity [0,1]

array a=(x ${y:~} z)   # y="" → a = [x, z] (2 elems); y="Q" → [x, Q, z]

n=${x:~}               # SB5046 — a scalar needs a value; use ${x:-}
echo ${args[0]}; ls ${args[0]:~}   # SB5047 — args[0] both required and optional
fn f { echo ${args[0]:~} ${args[1]} }   # SB5048 — required arg after optional
```

## Implementation notes
- Parse: `:~` in the parameter-expansion operator set, operand must be absent
  (SB4051).
- Arity (`transform/functions.rs`): `ArgSig` tracks required vs optional index
  sets (an index in both → SB5047); `analyze_arity` yields `[min, max]` and the
  trailing check (SB5048); the call-site check (`cmd_handlers.rs`) uses the
  range.
- Elision (`transform` → codegen): `${…:~}` in an elidable position lowers to a
  new `SsaValue::ElideEmpty(inner)`; the argv / array-literal builders expand it
  to 0-or-1 tokens (runtime identity branch, folded when the operand is a static
  literal or an absent arg). Native + c; janet follows or is filed.
- SB5046 (non-elidable `:~`) is a validate pass over expression positions.

## Deferred

The "additive-identity `0` for an int cell" clause of §(B) is **not yet wired** —
the first implementation (2026-08-31) elides on empty *string* (length 0), so an
int cell holding `0` stringifies to `"0"` and is not elided. The elision subject
is a string in every current use (args, string arrays); the int case is a rare
refinement. janet is deferred entirely (`janet-optional-args-elision`).

## Addendum (2026-09-02) — positional `${N:OP}` reached parity with `${args[N-1]:OP}`

DG hit a spurious **SB5042** on a function using `${2:~}` and called with one
arg. Two implementation bugs (the design always intended `$N` ≡ `${args[N-1]}`
— see §(a) "read even once as a bare `${args[i]}` / `$N`"):

1. **Arity mis-analysis** (`transform/functions.rs::sig_scan_pexp`). The
   `args[k]` path checked the operator for `:-`/`:~` (→ optional), but the
   positional `${N:OP}` path fell through to `sig_record_name`, which marked
   `$N` **required** unconditionally. So `${2:~}` demanded 2 args → SB5042.
   Fixed by classifying a positional `${N:-}`/`${N:~}` as optional (mirroring
   the `args[k]` rule) via `sig_record_positional`.
2. **Wrong disposition subject** (`transform/expressions.rs::pe_source_value`).
   For a no-index expansion it read `SsaValue::Var(lookup("N"))` — a scalar
   variable literally named "N", which never exists — so **every** `${N:-D}`
   defaulted and every `${N:~}` elided even when `$N` was present. Fixed by
   routing a positional name through `positional_to_value` (→ `args[N-1]`).

`${N:?}` stays **required** (consistent with `${args[N-1]:?}`): too few args →
SB5042 at compile time rather than a runtime require-abort. Mixing bare `$N`
with `${N:~}` → SB5047, as for the `args` form. Specs: `optional_args.sh`
84.19–84.24. journal 234.

## Addendum (2026-09-04) — `:~` on an external array splat, and `@{a:-}` fix

DG: "for array splat to command, we allow `:-` and `:?`; `:~` should work too."
Then, on review: "I thought `@{a:-}` produced an empty element in a command."

Two coupled changes, landing together:

1. The array-splat disposition (DD#69) now accepts **`@{a:~}`** alongside
   `@{a:-WORD}` / `@{a:?WORD}`. `@{a:~}` **elides** — empty array → zero args —
   and (like the scalar `${x:~}`) takes **no word** (`@{a:~WORD}` → SB5033).
2. **`@{a:-}` (bare, empty word) now yields one EMPTY arg**, not zero. This
   matches the scalar `${x:-}` (always exactly one token — an empty string) and
   bash `"${a[@]:-}"`, and resolves this DD's own internal contradiction (it
   stated the "`:-` = exactly one token" principle, then claimed `@{a:-}` → zero
   args). So `:-` (one element) and `:~` (zero / elide) are now genuinely
   distinct and uniform across scalar and array.

Verified identical in all three backends (empty: `@{a:-}` → `[PRE][]`, `@{a:~}`
→ `[PRE]`). Parser-only change: the encoding is `has_word = operand.is_some()`,
so `:-` now always carries an operand (empty word → `Some("")` → one empty
element) and `:~` carries none (`None` → zero args); the transform/runtime/janet
paths already mapped `Some`→one-element and `None`→zero, so nothing downstream
changed. The "drop-if-empty" idiom migrates from `@{a:-}` to `@{a:~}` (e.g.
`cat @{e:~}` to read stdin on an empty array — wordsplit 22.18).

Impl: `parse/expr.rs::parse_splat_op`. Specs `glob_splat.sh` gs.4 (now one empty
arg), gs.20–24; `wordsplit.sh` 22.18 (migrated to `:~`). journal 241.

## Addendum (2026-09-28) — a runtime-variable count needs a variadic context

DG:

> to my mind, an elision, given it could remove an argument, can only work for
> a variadic context and not a fixed one. similarly, a splat should only work
> for variadic given it could be empty (could try to track provably present
> elements of the array, but edge cases make that a non-starter, I think)

This closes the hole that §(A) "Absent optional args at the call site" already
named without noticing: that section rests on a call passing fewer than `max`
args being **"known at compile time"**. A splat and a `:~` elide are each *one*
value at compile time that expands to 0..n tokens at run time, so a call
carrying one has no compile-time argc at all. Both directions were wrong:

- **Silent under-delivery.** `f one ${x:~}` into a `Fixed{2,2}` callee
  compiled and ran, with `${args[1]}` reading empty. The equivalent `f one`
  was correctly SB5042.
- **False rejection.** A splat lowered to a single `SsaValue`, so
  `arg_values.len()` counted it as **1** regardless of the array's size:
  `f @q` on a two-element `q` was rejected as "called with 1", and a
  three-element `q` said "called with 1" too.

### The rule

A splat or a `:~` elide may be passed to a user function only where the callee
accepts any number of arguments, and only when the call's **minimum** meets
that callee's floor. Otherwise **SB5087**.

Per-argument minimum, used to compute the call's floor:

| argument | contributes at least |
|---|---|
| an ordinary value | 1 |
| `${x:~}` | **0** |
| bare `@a` | **0** |
| `@{a:~}` | **0** |
| `@{a:-}` / `@{a:-W}` | 1 (2026-09-04 addendum: empty → one empty element) |
| `@{a:?W}` | 1 (empty aborts, so any continuing path has one) |

DG pre-rejected tracking provably-present elements, so `array q=(p d); f @q`
into a fixed-arity callee is rejected even though the size is visible one line
above. The author writes the arguments out, or makes the callee variadic.

### This makes the elidable-position list uniform

§(B)'s three elidable positions are all **variadic contexts** — an external
command's argv (`execve` takes any argc), a sash varargs builtin, an array
literal element. A user-function call is the fourth, and is elidable exactly
when the callee is variadic. That is now the whole rule, rather than a
closed list that happened to omit the one position with a fixed arity.

### Consequence — `: @args` must not promote an optional index

Found while applying the above. `analyze_arity` computed a variadic function's
floor from `required ∪ optional`, so adding `: @args` — DD#81's documented way
to accept a variable number — turned a DD#84 *optional* argument back into a
required one:

```sash
fn f {           echo "[${args[0]:-}]" };  f    # accepted  (Fixed{0,1})
fn f { : @args;  echo "[${args[0]:-}]" };  f    # SB5042    -- wrong
```

The floor is now `required` only, matching the fixed path's `min`, which
already excluded optional-only indices. Specs 84.66/84.67.

### The fixed portion is POSITIONAL

DG, on reading the above:

> in my mental model, a function can have both a fixed portion and a variadic
> one with the fixed met by discrete arguments as though there were no
> variadic component. I grant that `@{e:-fallback}` can be guaranteed to
> provide 1 argument.

The first implementation summed the per-argument minimums above and compared
the sum to the floor. That is not enough, and the gap is a silent wrong
answer rather than a missing diagnostic:

```sash
fn f { : @args; echo "first=[${args[0]}]" }
array e=(q r)
f @e x            # accepted: 0 + 1 reaches the floor of 1
```

`${args[0]}` is `x` when `e` is empty and `q` when it is not — a fixed read
taking a different variable's value depending on an array's length, with
nothing said at compile time.

So the floor is about **positions**, not a count. Walking the argument list
left to right:

- a statically-one argument covers exactly its own position;
- a variable-length argument covers the positions its **minimum** guarantees
  (the table above), and after those **no later position is statically
  known** — a run-time length shifts everything behind it.

The call satisfies the callee iff positions `0..floor-1` are all covered
before that cutoff. This keeps DG's grant — `@{e:-fallback}` has a minimum of
one, so it covers position 0 — while rejecting `f @e x`, and it rejects
`f @{e:-fallback} y` against a floor of 2, since the guaranteed splat covers
only position 0 and `y`'s position moves with `e`.

The required arguments therefore come **before** the splat, which is the
ordinary shape anyway: `f a b @e`.

Cost against the existing suite: **zero specs**. Specs 84.68-84.72.

Impl: `transform/cmd_handlers.rs::check_callee_arity` + `call_arg_bounds`
(which is why the check now takes the argument *values*, not their count);
`transform/functions.rs::analyze_arity`. Specs `optional_args.sh` 84.53-84.67;
`functions.sh` sp.3 migrated. journal 443.
