---
title: String-array/map cell dispositions — `${a[i]:-N}` / `${a[i]:?msg}`
date: 2026-08-19
---

# String-array cell dispositions: parity with a string variable

DG, on the string-array cell in arith: **"I would expect the same behavior as
from a string variable."** Then, after asking me to build a rigorous ask-vs-
proceed decision tree: **"implement it properly."** This session does both.

## What shipped

A cell `${a[i]:OP}` now behaves exactly like a scalar `${s:OP}`, in both word
and arith contexts:

- `${a[i]:-N}` / `${a[i]:?msg}` (array or map, literal or `$i` dynamic index)
  apply the DD#77 disposition to the **cell's** value — arith strict-int
  (`13`→13, `hi`→default/abort), word unset/empty semantics.
- A **bare** string cell `$(( a[i] ))` / `$(( ${a[i]} ))` (and a bare string
  scalar used as an int index `a[s]`) → **SB4030**, the same string→int
  crossing D1 rejects for a bare scalar string. Int-array cells (DD#76) read
  the raw i64, unaffected.
- Operator parity: `${a[i]#x}`/`${a[i]/x/y}` → SB4034, `${a[i]@U}` → SB4015
  (same as a scalar). Substring `${a[i]:o:l}` / sed `${a[i]:s/…/}` have no
  cell-source lowering yet → an explicit **SB4039** (never a silent drop),
  filed as a follow-up.
- DD#78 rides for free: a cell disposition's default is taint-checked
  (`${a[9]:-$(getenv X)}` → SB5020) and non-`random` (→ SB5035).

## The bug this closed

`parse/expr.rs::try_subscript` computed the text after `]` (e.g. `:?nan`) and
**discarded it**, returning a bare `Subscript`. So `${a[i]:-N}` silently read
the raw cell (atoi'd it in arith) and `${a[i]:?msg}` never aborted — a silent
miscompile. Zero-tolerance: closed.

## Representation (my call, per the decision tree)

- `ir::ParameterExpansion` gained `index: Option<Box<Expr>>` — `None` for a
  scalar `${s:OP}`, `Some` for a cell. `try_subscript` now routes the trailing
  operator through the same scalar parsers and attaches the index; only `:-`/
  `:?` carry a cell subject to codegen, the pruned families reject on their
  operator string exactly as for a scalar, and substring/sed get the SB4039
  cell sentinel.
- The SSA disposition nodes generalized from `var: VarRef` to
  `source: Box<SsaValue>` (`SsaValue::Var` for a scalar, `SsaValue::Subscript`
  for a cell). ~19 mechanical sites across native codegen, emit_c, taint, and
  SSA printing. `ParamExpansion` stayed var-based on purpose — arrlen/strlen
  genuinely need the array *name/slot*, so it is inherently var-rooted; the
  cell-string ops (substring/sed) need a different source and are the deferred
  gate.
- janet emits from the AST, so `emit_pe_default`/`emit_pe_require` read the
  cell via `emit_expr_subscript` when `pe.index` is set (word-context parity).

## Backends

native + c: full parity (16 spec checks each). janet: word-context works
(scx.7-11 all modes); **arith**-context cell disposition rides the pre-existing
`janet-arith-subscript-read` gap (scx.12-14 guarded `native,c`).

## Verification

spec `arithmetic.sh` scx.1-16 (plan 133→143). Full 3-mode sweep, latest-run-
per-test diff vs HEAD: **0 regressions, 0 gone, 42 new passing** (native 16,
c 16, janet 10). fmt + clippy clean; complexity gate clear on all touched
functions (`parameter_expansion_to_value` cc10/cog5, `subscript_operator_expr`
cc7/cog3, `try_subscript` cc6/cog4, all under 3×mean).

## Decision authority

DG flagged over-asking ("too much hand-holding") and asked for a rule. The
operative test I now apply: **ask only when a decision is both underdetermined
and consequential/irreversible; otherwise decide and report.** Apply a stated
principle to all its consequences (don't narrow); fix bugs without asking
permission to fix. Under that rule the representation choice and the staged
SB4039 sequencing were mine to make and report; the string-cell = string-var
end-state was DG's stated principle, applied in full.

## Follow-ups

- `arith-bare-subscript-followups` gate 4: substring/sed on a cell → scalar
  parity (needs a cell-string source).
- `janet-arith-subscript-read`: widen scx.12-14 once janet's value-arith reads
  the subscript.
