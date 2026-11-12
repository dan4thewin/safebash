title: Align parser with postmortem decisions
status: done
depends: []
gates: [tree-sitter-removal]
resolved: 2026-06-07
resolution: All 7 phases shipped in the tree-sitter-removal arc (status.md 2026-05-24 entry). Phase 1 Pratt arithmetic landed in parse/arith.rs (5 Pratt hits). Phase 2 Tok::Dash restored in parse/lexer.rs. Validator and taint reimplemented as IR walkers in validate.rs (~650 lines). Janet emitter retargeted to the new parser output. tree-sitter removed from compile path (no tree_sitter refs in sash/Cargo.toml or sash/src/). Single-parser architecture is in place.

# Align parser with postmortem decisions

Seven decisions reviewed 2026-05-24. Three must change, two corrections,
one subset restriction. This task executes all of them.

## Phase 1: Pratt parser for arithmetic

**Dependency**: None
**Scope**: ~300 lines in `parse/arith.rs`. Replace raw-text collection
with structured AST (BinOp, UnaryOp, Ternary, VarRef, Literal, FuncCall).
Add spans. Transform receives structured nodes instead of re-parsing text.
**Fixes**: Spec test 58.3 (compound sexprs)
**Deletes**: Transform's text-based arithmetic re-parser

## Phase 2: Restore `Tok::Dash` with adjacency

**Dependency**: None
**Scope**: ~20 lines. Scanner emits `Tok::Dash` when `-` is followed by
whitespace/terminator. Adjacent `-n` stays as `Word("-n")`. Parser uses
`Tok::Dash` for suppression prefix. Pratt parser uses it for
subtraction/negation.
**Fixes**: Cleaner IR for suppressed commands and arithmetic

## Phase 3: Drop compound-command redirects

**Dependency**: Phase 2 (parser changes shouldn't conflict)
**Scope**: Remove `maybe_wrap_redirects` for compound commands. Keep it
for simple commands (which still need redirects). Spec tests 14.4–14.6
become rejection tests or are removed.
**Simplifies**: Parser, codegen

## Phase 4: Rewrite validator rules as IR walkers

**Dependency**: Phase 1 (arithmetic structure needed for SB5022)
**Scope**: ~1600 lines rewritten. 19 rules change from
`fn check(node: Node, source: &[u8])` to `fn check(program: &Program)`
or `fn check(stmt: &Statement)`. Run against new parser's IR.
**Pre-step**: Audit test coverage per rule. Add tests for any rule
with < 2 test cases.
**Deletes**: `sash/src/validator.rs` (tree-sitter walker framework)

## Phase 5: Rewrite taint as IR walker

**Dependency**: Phase 4 (same pattern, validator first proves approach)
**Scope**: ~975 lines → ~200 lines. Walk `ir::Statement`/`ir::Expr`
instead of tree-sitter nodes.
**Deletes**: `sash/src/taint.rs` (tree-sitter version)

## Phase 6: Rewire janet + oracle tests to new parser

**Dependency**: Phases 4+5 (tree-sitter gone from compile path)
**Scope**: `emit_janet.rs` takes `&Program` from `parse::parse()`.
Oracle tests call `parse::parse()` → `emit_janet()`. ~50 lines in tests.
**Deletes**: `sash/src/lower/` (3098 lines, 7 files)

## Phase 7: Remove tree-sitter dependency

**Dependency**: All above complete
**Scope**: Remove `tree-sitter`, `tree-sitter-bash` from `Cargo.toml`.
Delete `parse()`/`parse_bash()` from `lib.rs`. Remove logos (unused).
Clean dead code warnings.
**Result**: Single parser, no external grammar dependency.

## Ordering

Phases 1–3: independent parser improvements (any order).
Phase 4: gate to removing tree-sitter. One rule at a time, tests
passing between each.
Phase 5: follows same pattern as 4.
Phase 6: cleanup once tree-sitter gone from validation/taint.
Phase 7: final cut.

## Risk

Phase 4 (validator rewrite) is where regressions are most dangerous —
security rules. Mitigation: audit coverage first, add tests, then
rewrite one rule at a time.
