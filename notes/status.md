---
title: Status
modified: 2026-05-24
---

## Current branch: hand-parser

## Spec status: 274 passing / 7 failing (spec.db)

### Session 2026-05-24 — completed

**Postmortem review of parser migration decisions**
- Reviewed all 7 decisions made during the parser migration session
- Added addenda to notes/parser-migration-postmortem.md
- Created notes/dd-candidates.md for compound-command redirect question

**Phase 1: Pratt parser for arithmetic**
- Structured ArithExpr AST in ir.rs (12 variants, 19 binary ops, 4 unary ops)
- Separate arithmetic tokenizer + Pratt parser with correct precedence
- Sexpr detection for DD#58 function calls: (name arg1 arg2)
- Transform maps ArithExpr → SsaValue directly (no text re-parsing)
- Eliminated stringify-then-reparse round-trip for mutations
- Added StmtKind::ArithTest — arithmetic conditions without [[ ]] numeric ops
  (aligns with SB5019 which rejects -gt/-lt/-eq/-ne in [[ ]])

**Phase 2: Restore Tok::Dash with adjacency**
- Scanner emits Tok::Dash when '-' followed by whitespace/terminator
- Adjacent '-n', '-f' stay as Word("-n") via scan_word
- Parser uses Tok::Dash for suppression prefix
- Arithmetic subtraction handled by arithmetic tokenizer (ATok::Minus)

**Phase 3: Drop compound-command postfix operators**
- All postfix operators on compound commands rejected (SB4021–SB4024)
- Redirects, pipes, &&, ||, & on if/while/for/case/group
- Sash idiom: wrap in a function for composition
- Converter task filed for rewrite handling

### Test suite (no unintended regressions)
- codegen: 185/264 (5 fewer = intentional rejections)
- e2e: 97/124
- oracle: 59/59
- taint: 38/42
- spec: 274/281

### Remaining work (phases 4–7)
- Phase 4: Rewrite 19 validator rules as IR walkers (~1600 lines)
- Phase 5: Rewrite taint as IR walker (~975→200 lines)
- Phase 6: Rewire janet + oracle tests to new parser, delete lower/ (3098 lines)
- Phase 7: Remove tree-sitter dependency entirely

### 7 spec failures (unchanged)
- 21.2, 55.11: Suppressed pipeline exit code
- 39.2, 39.5: Regex mutation `x =~ s/pat/rep/`
- 39.6, 39.7: Regex in `[[ =~ ]]` validation
- 58.3: Sexpr arithmetic (nested function calls)
