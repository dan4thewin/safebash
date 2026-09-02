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
  analog of the array-splat elision (`@a` / `@{a:-}` on an empty array → zero
  args, DD#69/#75).

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

Arrays already had the missing capability: a bare `@a` (or `@{a:-}`) on an empty
array contributes zero args. `:~` brings the same "empty → disappears" to a
scalar, and `:-`/`:~` on a trailing `args[i]` make it genuinely optional.

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
