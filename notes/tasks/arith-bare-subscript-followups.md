---
title: "arith bare-subscript follow-ups: $a[i] form, nested index, string-array cell"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`$(( $a[i] ))` (dollar, non-brace) in arith reads the cell (model B table lists it as an rvalue read) — currently SB4035 `invalid numeric literal '['`"
  - "nested index `$(( a[a[0]] ))` parses (currently SB4035)"
  - "string-array cell in arith: decide DD#77-consistency (should `$(( a[i] ))` on a *string* array need a disposition like a bare scalar string, or silently atoi as today?)"
  - "no regression"
---

## Context

arith-subscript model B Stage 2 shipped bare `a[i]` (int-array cell,
rvalue+lvalue) via a `BareSubscript` lexer token + `ArithExpr::Subscript` /
`SubscriptAssign`. Three edges remain:

### 1. `$a[i]` dollar form in arith → SB4035

Model B's table lists `$a[i]` (non-brace) as an rvalue read "as today". Inside
`(( ))` it doesn't parse: `scan_arith_dollar` reads `$a` as `DollarVar("a")`,
then `[` hits `match_arith_op1` → unknown → `BadLit` → SB4035 (a confusing
`invalid numeric literal '['`). Bare `a[i]` and braced `${a[i]}` both work, so
there's a path; this is a convenience gap + a poor diagnostic. Fix: extend
`scan_arith_dollar` to capture `$name[…]` like `scan_arith_ident` does, or at
minimum give a clear "use `a[i]` or `${a[i]}`" error.

### 2. Nested index `a[a[0]]` → SB4035

`scan_balanced_brackets` captures the balanced run, and the parser re-parses the
inner text as arith — but `a[a[0]]` still rejects. Likely the arith-content
extractor (`collect_arith_content`) or a re-parse edge. Rare; fix when touching
the arith subscript parser.

### 3. string-array cell in arith silently atoi's

`array a=(hi 13 21); $(( a[0] ))` → 0 (atoi "hi"), no error. This matches the
pre-existing `${a[i]}` (Stage 1) behavior and the string-cell-in-arith path, but
it's in tension with DD#77 D1 (a bare *scalar* string in arith is SB4030 —
needs a disposition). Decide whether a string-array cell in arith should
likewise require a disposition / be rejected, or keep the lenient atoi. **DG
design call.** (Int arrays — the model-B target — read the raw i64, no atoi, so
they're unaffected.)

## Provenance

Filed during arith-subscript Stage 2 + B2 (journal 150). None block the shipped
int-array feature.
