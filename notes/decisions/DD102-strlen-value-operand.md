title: DD#102 — `strlen` takes a value, like every other scalar operation
status: implemented
depends: [DD75, DD65, DD100]

1. `strlen` measures the byte length of a scalar string value, not a named
   variable or collection. A bare word is literal text; expansions supply values.
2. Collection splats are not implicitly joined. Integer operands require
   explicit string conversion (DD105); ordinary taint checks apply.
3. Capture and arithmetic-call spellings preserve the same operand meaning,
   including the distinction between a bare word and a dollar-prefixed read.

---

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
- ~~An int variable still silently stringifies (`strlen $n` → 2, matching
  `toupper $n` → "42"). DG has separately directed that the string intrinsics
  reject int variables; that is a follow-up across the whole family, not a
  `strlen` special case.~~ DD105 requires explicit string conversion.

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

## An unfinished consequence, closed 2026-09-17 (journal 353)

This decision moved `strlen` from the name-taking family (`len`/`join`/`keys`)
to the value-taking one (`toupper`/`sub`/`match`). **SB5070's operand set was
not moved with it.**

SB5070 rejects an array splat in a scalar operand slot, and its comment says
`len`/`join`/`keys` are "deliberately absent: they take a collection by name,
so `@a` there is not a silent reinterpretation". That reasoning stopped
applying to `strlen` the moment this DD landed — but the list, keyed on the
family `strlen` had just left, kept it out. `type_check.rs::string_operand_slots`
*was* updated and groups `strlen` with `toupper`.

So `strlen @a` remained the one silent join in the language: it quietly meant
`strlen "$(join a ' ')"`, returning 6 for `array a=(12 345)`. Every sibling
rejects it — `toupper @a` is SB5070, `len @a` and `join @a` are SB5062.

DG, 2026-09-17: *"strlen should only work on scalars, not collections."*
`strlen` added to `is_scalar_operand_intrinsic`; `strlen @a` is now SB5070,
which already names the right remedy (`$(strlen "$(join a SEP)")`).

**A second defect surfaced with it**, pre-existing and affecting all eight
operators: SB5070 fired on *any* splat-shaped argument, so `toupper @s` on a
scalar asserted "`@s` is an array splat" and advised `join s SEP` — which is
itself rejected. It is now gated on the subject actually being a collection
(declarations, bulk-fill targets, DD#100 slots, `args`), deferring a scalar
subject to SB5079 and an unintroduced one to SB5031. Specs `75.88`,
`75.90`–`75.93`.

The general form, worth keeping: **a decision that moves an operator between
families has to move it in every list keyed on the old family.** One list was
updated, one was not, and the gap lived five days.

## See also

- DD#75 §2 — split `len` (elements) from `strlen` (bytes)
- DD#65 — the string sugar (`toupper`/`sub`/`match`) whose operand discipline
  this adopts
- DD#100 Part D — a builtin's *name* operand takes a bare name and no splat;
  still true of `len`/`join`/`keys`
- `notes/tasks/strlen-cell-operand-spelling.md` — dissolved by this decision
- SB5079 / `notes/tasks/bug-scalar-splat-segfault.md` — splatting a
  non-collection; SB5070 now defers to it
