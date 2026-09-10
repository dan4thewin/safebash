---
title: "indexnudge enum_variant_refs shipped — dead-variant audit (HereDoc + 4 more)"
status: done
priority: P3
kind: chore
depends: []
gates:
  - "RedirectOp::HereDoc removed (tracked separately in finish-heredoc-removal.md)"
  - "ArithOp::Not removed: variant + its 4 match arms (codegen/arithmetic.rs:34, ssa.rs:425/450, transform/expr_handlers.rs:408); unary `!` already lowers via IrUnaryArithOp::Not"
  - "SsaInst::Phi removed (or a TODO filed): variant + 3 arms (codegen/dispatch.rs:53, emit_c.rs:327, ssa_taint.rs:164) — only if phi insertion is genuinely not planned"
  - "StmtKind::Subshell and TestExpr::Paren: confirm dead vs deferred-feature, then remove or annotate"
  - "spec suite stays green after each removal (source of truth)"
---

# Response to the dead-code FR: `enum_variant_refs` is shipped

This is the response to the feature request that originated here
(safebash `RedirectOp::HereDoc`) and was filed upstream as indexnudge
`notes/feature-request-dead-code-detection.md`.

## Status: implemented (indexnudge schema v11)

The requested view exists. Re-index with the current indexnudge Rust
binary (schema auto-rebuilds on `--freshen` when the version is stale)
and the view is available:

```bash
# Dead variants: referenced (matched/compared) but never constructed
sqlite3 .indexnudge-rust.db "
  SELECT v.enum, v.variant
  FROM (SELECT DISTINCT enum, variant FROM enum_variant_refs) v
  WHERE NOT EXISTS (
      SELECT 1 FROM enum_variant_refs c
      WHERE c.enum = v.enum AND c.variant = v.variant
        AND c.context = 'construct');"

# Every cleanup site for a specific variant
sqlite3 .indexnudge-rust.db "
  SELECT file, line_num, context FROM enum_variant_refs
  WHERE enum = 'RedirectOp' AND variant = 'HereDoc'
  ORDER BY file, line_num;"
```

`enum_variant_refs(file, line_num, symbol, enum, variant, context)` —
`context` is `construct` / `match_pat` / `compare`. It's a pure-SQL view
over `lines` (zero storage, same family as `string_literals`), restricted
to locally-defined enums (`symbols.kind='enum'`).

## The FR predicted "exactly one row" — it actually finds five

The dead-variant query returns **5** candidates, not 1. After verifying
each by hand against the source, **all five are genuinely never
constructed**:

| enum         | variant   | match/compare sites | evidence it's dead |
|--------------|-----------|--------------------:|--------------------|
| `RedirectOp` | `HereDoc`  | 8 | the known case — already tracked in `finish-heredoc-removal.md` |
| `ArithOp`    | `Not`      | 4 | unary `!` lowers via the *separate* `IrUnaryArithOp::Not` (parse/arith.rs:724); `codegen/arithmetic.rs:34` is `panic!("ICE: unary ! in binary arithmetic context")`. The `IrArithOp → ArithOp` map only covers binary ops. |
| `SsaInst`    | `Phi`      | 3 | phi nodes are never inserted; `SsaInst` is built via `.push(SsaInst::Test/Bless/...)` but never `Phi`. `ssa_taint.rs:164` even comments "handled above". |
| `StmtKind`   | `Subshell` | 8 | `StmtKind` is constructed via `StmtKind::If/Redirected/...` but never `Subshell`; the variant is only ever matched. |
| `TestExpr`   | `Paren`    | 10 | the test parser builds `TestExpr::Or/And/Unary` but never `Paren` (parens are flattened); only matched. |

How the verification was done (so the numbers are trustworthy): each of
these enums **is** constructed elsewhere via the qualified
`Enum::Variant` form — which the view detects as `construct` — so the
view sees their construction idiom. These specific variants simply never
appear in a construct position. There are no glob imports
(`use Enum::*`) of these enums, so no bare-variant escape hatch, and no
`Self::Variant` constructions hiding inside an impl.

A nice precision check: the view flags `TestExpr::Paren` (dead) while
correctly leaving `ArithExpr::Paren` **off** the list — it's constructed
at `parse/arith.rs:792` (`ArithExpr::Paren(Box::new(expr))`). Same
variant name, different enum, correctly distinguished.

So the FR's "exactly one row" was an incomplete expectation: beyond
HereDoc, safebash carries **four more vestigial enum variants**. This is
the tool working — it generates high-precision candidates; a human (this
audit) confirms before deletion.

## Two safebash idioms that the implementation had to handle

These are why you can trust the count (the first cut returned 29 false
positives before they were fixed):

1. **`Self::Variant`.** `BuiltinCmd::from_name` builds all 24 builtins
   via `Self::Echo`, `Self::Test`, … The view resolves `Self` to the
   enclosing type from the attributed symbol, so none of the `BuiltinCmd`
   variants are falsely flagged.
2. **Fully-qualified paths.** `ssa_taint.rs:179` compares
   `r.op == crate::ir::RedirectOp::HereDoc` and `validate.rs:1829`
   matches `… | crate::ir::RedirectOp::HereDoc`. Both are detected
   (the `compare` row for HereDoc comes from ssa_taint). Without this,
   HereDoc itself would have been missed.

## Suggested action

- **HereDoc**: already covered by `finish-heredoc-removal.md` — no new
  work here, just cross-referenced.
- **ArithOp::Not**: strongest case (ICE panic + separate unary path).
  Safe to delete the variant and its 4 arms.
- **SsaInst::Phi / StmtKind::Subshell / TestExpr::Paren**: confirm each
  is "vestigial" vs "scaffolding for an unimplemented feature" (subshells
  and phi nodes especially may be intended future work). Remove if dead,
  or annotate/track if deferred.

Re-run the query after any IR change; it's a standing audit, not a
one-shot.

## Discovered
2026-06-22, validating the indexnudge `enum_variant_refs` feature against
this repo's index — the feature was built to answer exactly the HereDoc
question this repo raised, and surfaced four more dead variants in the
process.

## Completed (2026-06-22) — all 5 removed, one commit each, native spec green

| variant | commit | scrutiny note |
|---|---|---|
| `SsaInst::Phi` | 6b28253 | phis live in `block.phis`; inst-arm was a no-op |
| `TestExpr::Paren` | 6b0a6b6 | parens flattened at parse; cascaded to dead `SsaTest::Paren` |
| `ArithOp::Not` | 7aea404 | unary `!` uses `IrUnaryArithOp::Not`; removed the ICE panic arm |
| `RedirectOp::HereDoc` | e639cf3 | heredocs dropped (SB5002); closed finish-heredoc-removal |
| `StmtKind::Subshell` | 96c48e6 | DD#38 rejects subshells; cascaded to `Subshell` struct, `lower_subshell_stmt`, `BuiltinCmd::SubshellWait`, `emit_subshell*` |

**Corrections to the original task's claims** (per DG's "scrutinize"):
- It implied each was a tiny removal; **Paren and Subshell cascaded** to a
  second dead variant each (`SsaTest::Paren`, `BuiltinCmd::SubshellWait`)
  that the view didn't flag because of **transitive deadness** (their lone
  construct site is fed only by the variant being removed).
- `BuiltinCmd::SubshellStart` is **NOT dead** — it's the live fork primitive
  for background `cmd &` (DD#38). Kept; doc-comment corrected.
- The "dead vs deferred" question is resolved: all five are genuinely dead
  (verified against DD#38, the `block.phis` mechanism, paren flattening,
  and the `IrUnaryArithOp::Not` path), not deferred-feature scaffolding.

`enum_variant_refs` now reports zero dead variants. It's a standing audit —
re-run after IR changes.
