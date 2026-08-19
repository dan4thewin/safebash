---
title: "arith bare-subscript follow-ups: $a[i] form, nested index, string-array cell"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`$(( $a[i] ))` (dollar, non-brace) in arith reads the cell (model B table lists it as an rvalue read) — currently SB4035 `invalid numeric literal '['`"
  - "nested index `$(( a[a[0]] ))` parses (currently SB4035)"
  - "substring `${a[i]:o:l}` / sed `${a[i]:s/…/}` on a cell (currently the explicit SB4039 staged-reject) reach scalar parity — needs a cell-source lowering for substring/sed"
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

### 3. string-array cell in arith — RESOLVED (2026-08-19)

`array a=(hi 13 21); $(( a[0] ))` now rejects with **SB4030** (a bare string
cell is the same string→int crossing as a bare scalar string — DD#77 D1). The
disposition escape hatch `${a[0]:-N}` / `${a[0]:?msg}` works (native,c; janet
word-context). DG: "same behavior as from a string variable." See DD#77's
"Implemented — string-array/map cells" section + journal 151.

### 4. substring / sed on a cell → SB4039 (staged)

`${a[i]:o:l}` (substring) and `${a[i]:s/…/}` (sed) work on a scalar string but
have no cell-source lowering yet, so they reject **explicitly with SB4039**
(never a silent drop). Full parity needs a cell-string source for the
`ParamExpansion`-substring path and the `:s/` sed-subject construction. The
pruned families (`#`/`%`/`/`/`@X`) already reject at scalar parity (SB4034/
SB4015). janet **arith**-context cell disposition rides the
`janet-arith-subscript-read` gap.

## Provenance

Filed during arith-subscript Stage 2 + B2 (journal 150). None block the shipped
int-array feature.
