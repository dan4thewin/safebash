title: DD#77 — String→int conversion discipline in arithmetic
status: accepted

Crossing the string→int boundary inside `(( ))` / `$(( ))` / `if (( ))` must be
**explicit and accountable**. A string operand carries a required failure
*disposition* (borrowed from DD#69's array-splat `@{a:-WORD}`/`@{a:?WORD}`), and
a new `[[ -i … ]]` test lets an author validate/branch before converting.
Decided 2026-08-17 (DG).

## Motivation

DD#30 says conversion is explicit at boundaries ("`$(( ))` for string→int").
DD#72 made *parse-format* leniency (leading zeros, `0x`/`0o`/`0b`) the rule for
runtime data — but that was never a blank check to swallow a *failed*
conversion. Today `s=hello; $(( s + 1 ))` silently yields `1` (atoi→0), the last
silent-miscompile class in arithmetic (DD#72 already closed the constant side).
`atoi` already aborts on bad input — arithmetic should be equally accountable.

This DD tightens DD#30 from "wrap it in `$(( ))`" to "wrap it *with* a
disposition", and repurposes the (currently silent-0, buggy — see
bug-arith-param-expansion-operator-zero) `${var:OP}` arith path as the vehicle.

## The rule

Inside any arithmetic context, an operand is one of:

- **int** — an `int` var, an `int array`/`int map` element `${a[i]}` (DD#76), a
  numeric literal, an int intrinsic/`-> int` capture (DD#58/Option C), or a
  sub-expression. Needs **nothing**.
- **string** — a string var, a string-collection element, etc. **Must** carry a
  disposition; a bare string operand is a **compile error** (D1).

### Disposition forms (D2)

    $(( ${s:-N}   + 1 ))   # s-as-int if valid, else the literal N
    $(( ${s:?msg} + 1 ))   # s-as-int if valid, else abort: "<prog>: msg"

The arith trigger for the disposition is **"not a valid integer"**, which
*unifies* three cases into one: unset, empty, or set-but-non-numeric. (In string
context `${s:-x}`/`${s:?x}` keep their unset-or-empty-only trigger — DD#65;
"valid" simply means "valid int" in arithmetic.)

### Validity — strict (D3)

A string is a valid integer iff it is a **complete** integer: optional sign then
decimal digits, or a `0x`/`0o`/`0b` prefix with valid digits (DD#72's success
grammar). Partial parses (`"42abc"`) and non-numeric (`"hello"`) are **invalid**
→ they route to the disposition. This removes the lenient-leading atoi surprise
from arithmetic entirely (there are no bare string operands left to atoi).
Empty/unset are likewise invalid → disposition.

### The default's type (D4)

In `${s:-N}` the default `N` must be a **numeric literal** (statically checked);
`${s:-hello}` in arith is a compile error. Guarantees the fallback is a real int.

## `[[ -i EXPR ]]` — integer-validity test

`[[ -i $s ]]` is true iff `$s`'s value is a valid strict integer (same grammar
as D3) **and it fits i64** (range-checked, DG 2026-08-17). Empty/unset → false;
a well-formed but out-of-range value (`> i64::MAX` / `< i64::MIN`) → false. So
`[[ -i $s ]]` means "valid AND representable" — a validated value is guaranteed
to convert without an overflow trap. A sash-specific unary test (bash has no
`-i`).

It lets an author validate and branch instead of (or before) converting:

    s=42abc
    [[ -i $s ]] || s =~ s/\D//g      # not an int? strip non-digits → "42"
    (( n = ${s:?bad count} ))        # now safe; disposition still required

**Composition model:** `-i` *validates/branches*; the disposition *converts*.
There is **no** flow-sensitive narrowing — after `[[ -i $s ]]` the compiler still
sees `s` as a string, so using it in arith still requires a disposition
(`${s:?}` / `${s:-N}`). `-i` and the disposition are complementary, not
substitutes: branch-beforehand vs handle-inline.

**Out-of-range conversion.** If an out-of-range value reaches a conversion
anyway (unvalidated), the runtime `sash_atoi` traps cleanly via
`sash_arith_overflow` (`<prog>: arithmetic overflow`, exit 2) — the same trap as
arithmetic overflow (DD#25/#26), not a Rust panic. (Range-checking `-i` and the
clean atoi trap were added together, 2026-08-17.)

## What does NOT change

- Int operands in arith — nothing required.
- String-context `${s:-x}` / `${s:?x}` — unchanged (unset/empty trigger, DD#65).
- DD#72's success-parse grammar (leading zeros, prefixes) still defines a *valid*
  integer; only the silent-0 *failure* path is replaced.

## Mechanism / bug convergence

The vehicle is the arith `${…}` parser. Today `braced_body_token`
(`parse/arith.rs`) classifies only `${#name}` and `${name[i]}`; every
`${var:OP…}` falls through to a bogus variable named `"var:OP…"` → undefined → 0
(bug-arith-param-expansion-operator-zero, a silent miscompile). The fix routes a
full `${…}` body through the **real parameter-expansion parser** (as the BUG-1
`${a[i]}` fix did, generalized) → `ArithExpr::Expansion` → `expr_to_value`, so
`${s:-N}`/`${s:?msg}` become genuine expansions carrying the DD#77 semantics.

Runtime adds a strict validator `sash_is_int(ptr,len) -> i64` (distinct from the
lenient `sash_atoi`), used by both `-i` and the arith disposition. c/janet
backends get the same or a filed per-backend guard.

## Scope / follow-ups (not decided here)

- **Subscript index** `a[$s]` and **string `$(cmd)`** in arith are related
  string→int sites; this DD governs the `(( ))`/`$(( ))` operand path. Whether
  the same disposition requirement extends to those is a follow-up.
- Only `:-` and `:?` dispositions (matching DD#69 splat); other operators
  (`:+`, `#`, `%`, `:s/`) in an arith operand position → compile error
  (a string-producing op still needs a disposition to become int).

## Resolved sub-decisions (2026-08-17, DG)

**`:?` un-prune scope — everywhere, mirroring `:-`.** `:?` was entirely
SB4034-pruned (DD#65). DD#77 un-prunes it in BOTH contexts, as the abort/default
pair with `:-`:
- **word context** `${s:?msg}` — abort on unset/empty (bash semantics), like a
  first-class expansion (not just an arith disposition).
- **arith context** `${s:?msg}` — abort on *not-a-valid-integer* (unset, empty,
  or non-numeric), per D2.

**D1 vs DD#72 — D1 as approved; DD#72's bare-coercion is superseded.** DD#72's
"lenient runtime string→int coercion" for a *bare* `$(( x ))` operand (its
`arith_literals.sh` block: `x=0xff`, `x=010`, `x=08`, …) conflicts with D1
(bare string in arith → compile error). D1 wins: those tests migrate to a
disposition (`$(( ${x:?} + 0 ))`), and DD#72 is amended — the lenient *success*
grammar (0x/0o/0b, leading zeros) still defines a *valid* integer (now consumed
by the disposition's `is_int`/atoi), but a bare string operand no longer
silently coerces; it must carry `:-`/`:?`.

**D4 — the `:-` default must be a *decimal* integer literal (for now).** In
`$(( ${s:-N} ))` the default `N` is lowered by a plain decimal `sash_atoi` over
its bytes, so a non-decimal default silently became `0` (`${s:-hello}`,
`${s:-0xff}`, `${s:-5+3}`, `${s:-}` all → 0). D4 (`check_arith_default_literals`,
type_check) rejects any default that is not a canonical decimal literal
(optional `-`, no leading zero) — SB4030. This closes the silent-0 footgun; a
hex/octal default is now a clear compile error rather than a silent 0. Widening
the default to the full DD#72 grammar (`0x`/`0o`/`0b`, which also needs the
lowerer to stop atoi-ing) is a filed follow-up
(`arith-disposition-hex-octal-default`), not a bug — no silent-wrong value
remains.

**Out-of-range input is a disposition concern, not an "arithmetic overflow".**
An input string that is numeric-looking but exceeds i64 (`9223372036854775808`)
is "not a valid integer" under `parse_strict_i64`'s range check, so `${s:?}`
aborts (validation) and `${s:-N}` defaults — it is NOT the DD#25 arithmetic-
overflow trap (that fires on an *operation* like `INT_MAX + 1`). The old
`arithmetic.sh` pex.7 (bare `$(( s ))` out-of-range → overflow trap) migrated to
the disposition form (`${s:?}` abort / `${s:-0}` default); genuine operation
overflow stays covered by 25.2–25.12.

## Staging

C1 — un-prune `:?` (word + arith), abort node + 3-backend codegen. **Done.**
C2 — arith strict-disposition semantics for `:-`/`:?` (sash_is_int gate). **Done.**
C3 — D1 bare-string-in-arith error + D4 numeric-literal default + DD#72 test
migration + DD#72 amendment. **Done** (native,c; janet arith-disposition gap
guarded, `janet-arith-subscript-read`). DD#77 complete.

See notes/tasks/dd77-arith-string-conversion.md.


## Implemented — string-array/map cells (2026-08-19, DG)

DD#77's D1 rule already covers "a string-collection element" — this session
makes the cell disposition real. **DG: "re: string-array cell; I would expect
the same behavior as from a string variable."** So a cell `${a[i]:OP}` behaves
exactly like a scalar `${s:OP}`, in both word and arith contexts.

- **Disposition on a cell now works.** `${a[i]:-N}` / `${a[i]:?msg}` (array or
  map, literal or dynamic index) apply the DD#77 semantics to the *cell's*
  value: arith strict-int (`13`→13, `hi`→default/abort), word unset/empty. This
  closes a **silent-miscompile bug** — `parse/expr.rs::try_subscript` dropped
  the operator after `]`, so `${a[i]:-N}` silently read the raw cell (atoi) and
  `${a[i]:?msg}` never aborted.
- **Bare string cell → SB4030.** `$(( a[i] ))` / `$(( ${a[i]} ))` on a *string*
  array (and a bare string scalar used as an int index `a[s]`) is the same
  string→int crossing as a bare scalar string → D1 error, needing a
  disposition. Int-array cells (DD#76) read the raw i64, unaffected.
  This includes the implicit string array `args`: bare `args[i]` is not an
  integer conversion. Installing its environment in every type-checking
  visitor closed that bypass (2026-10-18, specs `bsx.8`/`bsx.9`).
- **Operator parity.** The pruned families reject identically to a scalar:
  `${a[i]#x}`/`${a[i]/x/y}` → SB4034, `${a[i]@U}` → SB4015. Substring
  (`${a[i]:o:l}`) and sed (`${a[i]:s/…/}`) have no cell-source lowering yet —
  they reject **explicitly** with **SB4039** (never a silent drop) and are a
  filed follow-up (`arith-bare-subscript-followups`).
- **DD#78 default discipline rides for free.** A cell disposition's default is
  still taint-checked (`${a[9]:-$(getenv X)}` → SB5020) and non-`random`
  (`${a[9]:-$(random)}` → SB5035).

Representation: `ir::ParameterExpansion` gained an optional `index`; the SSA
disposition nodes generalized from `var: VarRef` to `source: Box<SsaValue>`
(a `Var` for a scalar, a `Subscript` for a cell). native + c reach full parity;
janet word-context works, janet **arith**-context cell disposition is the
pre-existing `janet-arith-subscript-read` gap. Journal 151.
