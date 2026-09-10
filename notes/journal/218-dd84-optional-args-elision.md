## 218 — DD#84: optional trailing args (`:-`/`:~`) + the elide operator `:~`

**On `hand-parser`.** Native + c. Adds two coupled capabilities to the
args/positional model (DD#84, amends DD#75 §5 + DD#81).

### Motivation (DG)

sash doesn't word-split, so `${x:-}` always expands to *exactly one* token (an
empty string), never nothing — `echo a ${x:-} b` is `a "" b`, `array c=(x ${y:-}
z)` is `[x, "", z]`. And `${args[i]:-}` didn't make the arg optional (arity still
required it, SB5042), so the default could only fire from a caller passing an
explicit empty string. Arrays already elide (`@a`/`@{a:-}` on an empty array →
zero args); `:~` brings that "empty → disappears" to a scalar, and `:-`/`:~` on a
trailing `args[i]` make it optional.

### (A) Optional trailing args

An `args[i]` read *only* via `:-`/`:~` is optional; a bare read makes it
required. Arity becomes `[min, max]` (`min` = highest required + 1, `max` =
highest read + 1). New rules: mixing an index bare + dispositioned → **SB5047**;
a required arg after an optional one → **SB5048** (optional must be trailing).
`transform/functions.rs`: `ArgSig` now tracks `required`/`optional` index sets;
`analyze_arity` returns `Arity::Fixed { min, max }` + a `Vec<ArityDefect>`
(Hole/Mixed/RequiredAfterOptional) emitted at the definition; the call-site check
(`cmd_handlers.rs`) uses the range. Absent optional args work for free — a
function inlines per call with the exact args (DD#75 unique-name model), so a
short call materializes a short `args` array and a dispositioned read of an
unmaterialized index resolves per its operator.

### (B) The elide operator `:~`

`${x:~}` / `${args[i]:~}` — in an elidable position, contributes **zero tokens**
when its value is empty (identity) and one token otherwise. Never takes a word
(`${x:~foo}` → **SB4051**, parse). Only valid in an *elidable position* — a
command/builtin arg or an array-literal element; anywhere a value is required
(scalar RHS, `[[ ]]` operand, nested in a concat, …) → **SB5046** (validate,
`check_elide_positions`, a position-aware walk that resets to non-elidable inside
Concat/DoubleQuoted/Test/Subscript).

Lowering: `:~` → `SsaValue::ElideEmpty(source)` (transform). The list builders do
a runtime 0-or-1 on emptiness — this handles absent-arg, passed-empty, and a
runtime var *uniformly* (no compile-time special-casing needed):
- native argv: `store_argv_entry_if_nonempty` (argv over-allocated, NUL at the
  real count); native array: `lower_indexed_array_value` rewritten to a running
  write index with a conditional set (empty element skipped, indices compact).
- c: `emit_argv_splat` guards each elide store `if (len != 0)`; `emit_indexed_array`
  uses a running `_w` index with a guarded set.
A scalar `lower_value`/`lower_value_as_str` reaching `ElideEmpty` is a loud ICE
(validate's SB5046 guards it).

### Verify

specs `spec/optional_args.sh` (84.1–18): optional arity via `:-` (0/1/too-many),
required+optional mix arity [1,2], `:~` elision (absent/present/passed-empty/
runtime-empty/runtime-nonempty), array-literal elision (compact + kept), and the
rejects (SB4051 / SB5046 / SB5047 / SB5048). Full 3-mode sweep vs `aea691e`: **0
regressions; +18 native, +18 c, +4 janet** (the risky arity refactor touched
every function definition — clean). 16 lib unit tests pass. fmt/clippy/complexity
clear.

### janet (deferred)

janet has no `args`-array model (`${args[i]}` → "unknown symbol args") and
doesn't elide `:~` (emits the empty token). Positive specs gate `native,c`; the
parse/validate rejects (SB4051/SB5046) run all modes. Filed
`janet-optional-args-elision`.

### int-cell identity (noted)

The DD's "additive-identity `0` for an int cell" clause is not yet wired — `:~`
elides on empty *string* (len 0). An int cell value `0` stringifies to `"0"`
(len 1) and is not elided. Rare in a list; a refinement, not tracked separately
(the elision subject is a string in every current use).
