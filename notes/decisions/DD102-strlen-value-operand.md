title: DD#102 — `strlen` takes a value, like every other scalar operation
status: implemented
depends: [DD75, DD65, DD100]

Settled with DG 2026-09-12.

`$(strlen X)` resolves its operand as a **value**, lowered exactly as
`$(toupper X)` lowers its own. It no longer resolves a name.

| operand | before | after |
|---|---|---|
| `strlen hello` | SB5039 "introduced nowhere" | **5** |
| `strlen s` (`s=hello`) | 5 | **1** — the string `"s"` |
| `strlen $s` | 5 | 5 |
| `strlen "$s"` | SB5062 | **5** |
| `strlen ${a[0]}` | SB5062 | **the cell's length** |
| `strlen $(echo hi)` | SB5062 | **2** |
| `strlen a` (a is an array) | SB4030 "needs a scalar" | **1** — the string `"a"` |

## The decision

DG: *"strlen should behave like toupper."*

That sentence is the decision, and it was prompted by a measurement. Every
non-collection operation in the language already took a value:

| op | `hello` | `s` | `$s` | `"$s"` | `${a[0]}` |
|---|---|---|---|---|---|
| `echo` | hello | **s** | hello | hello | world |
| `toupper` | HELLO | **S** | HELLO | HELLO | WORLD |
| `match … "h"` | y | **n** | y | y | n |
| `[[ -n X ]]` / `[[ i? X ]]` | value | value | value | value | value |
| **`strlen`** *(before)* | SB5039 | **5** | 5 | SB5062 | SB5062 |

`strlen` was the exception, and the inconsistency was visible at arm's
length: `toupper s` yields `"S"` — the one-character word, upper-cased —
while the adjacent `strlen s` asked about the *variable* `s`. Two operations
on scalars, identical spelling, opposite operand discipline.

So this is not a new design. It removes an exception.

## What this dissolves

**`strlen-cell-operand-spelling` (was `status: question`).** That filing asked
how a cell should be spelled as a `strlen` operand and laid out four options,
recommending "A: carve `Cell` out of SB5062 for `strlen` only". The question
disappears rather than being answered: `${a[0]}` already parsed as an
`ExprKind::Subscript` and was rejected *only* because `classify_operand`
bucketed it as "not a name". Once the operand is a value, a cell interpolates
like any other value, with no carve-out, no new parsing precedent, and no
"one foot in each camp" — the cost the filing correctly identified in its own
recommendation.

It also unblocks **`bug-convert-hash-array-conflation`**, which needed
`${#a[i]}` → a per-element byte length to convert bash's `${#a}` correctly.

**An ICE.** `strlen` on an int variable panicked the compiler at
`codegen/types.rs:29` ("ICE: expected string var, got int"), because the name
path demanded a *string* var slot. A value operand has no slot to mismatch.

**A Janet gap.** `spec/types.sh` 75.58 was gated `native,c` because Janet
emitted a `$`-sigilled operand verbatim, reaching the script as a bare symbol.
Janet's `strlen` now lowers through `emit_expr_inline` like `toupper`, so the
gate is lifted.

## Consequences

- **Breaking**, and deliberately so: `strlen s` changes from 5 to 1. Twenty-
  seven spec sites migrated to `strlen $s`. There are no callers in
  `sash/tests`, `docs`, `contrib`, `suites`, or `sash-convert`.
- `len` is **unchanged** — it still resolves a name, because a collection
  cannot be a literal. `len` and `strlen` no longer share an operand
  discipline, which is the point: they no longer ask the same kind of
  question. DD#75 §2 split *bytes* from *elements*; this splits *value* from
  *name* along the same seam.
- SB5039 and SB5062 no longer fire for `strlen`. An unknown bare word is a
  string literal, not a missing variable, so there is nothing to report.
- The `len`-is-a-scalar advice now reads ``use `$(strlen $s)` `` — the old
  text recommended `$(strlen s)`, which under this decision answers `1`.
- Taint is **unchanged**, verified both ways: the old path reached
  `taint_of_var` through `ParamExpansion`, and the new one reaches
  `any_tainted` through the operand, so `strlen` of a tainted value is
  tainted in both. `strlen $(getenv HOME)` is SB5020 before and after.
- An int variable still silently stringifies (`strlen $n` → 2, matching
  `toupper $n` → "42"). DG has separately directed that the string intrinsics
  reject int variables; that is a follow-up across the whole family, not a
  `strlen` special case.

## The arith sexpr operand

`reconstruct_intrinsic_command` rebuilt a `Command` from sexpr args and
collapsed `ArithExpr::Var` and `ArithExpr::DollarVar` into a single
`ExprKind::Literal`. That was lossless while *every* int intrinsic resolved a
name — both spellings named the same variable — and silently wrong the moment
one of them took a value: `$(( (strlen $s) ))` measured the name and answered
1. The rebuild now preserves the distinction (`Var` → `Literal`,
`DollarVar` → `SimpleExpansion`). Pinned by 75.59 and 75.60.

This is the thirteenth instance of the recurring class — *a value that could
not be resolved resolved to a plausible constant instead of failing* — and
the second in this batch to come from one concept having two spellings with a
guard on only one.

## See also

- DD#75 §2 — split `len` (elements) from `strlen` (bytes)
- DD#65 — the string sugar (`toupper`/`sub`/`match`) whose operand discipline
  this adopts
- DD#100 Part D — a builtin's *name* operand takes a bare name and no splat;
  still true of `len`/`join`/`keys`
- `notes/tasks/strlen-cell-operand-spelling.md` — dissolved by this decision
