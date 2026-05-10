---
title: Status
modified: 2026-05-10
---

## Current branch: split-converter

## Priority: autonomous agent infrastructure + DD#31/DD#55/DD#58

### Just completed (session 2026-05-09/10)

**Autonomous agent batch run — 10/10 tasks resolved:**
- 9 merged automatically (reject-globstar, reject-extglob, implement-failglob,
  dd23-remove-command-v, dd23-remove-path-walk, heredoc-cmdsub,
  dd49-bless-anti-tautology, dd55-dollar-question-scoping,
  remove-implicit-env-fallback)
- 1 fixed manually (dd31-reject-mutation-in-expr — tree-sitter comma parsing)

**Design decisions:**
- DD#58: integer function capture via s-expression syntax in $(( )) and (( ))
- DD#17: updated — $(f) is string-only, $(f) for -> int is a type error
- DD#55 non-void functions: SB4016 (bare call), SB4017 (return-0-only)

**Infrastructure:**
- run-compiler-tasks.sh autonomous runner: args mode, SPEC_TSV gate checking,
  safe auto-commit, compile error detection, all-spec-file scanning
- .goosehints: code index docs (indexnudge-rust.db)
- spec/common.sh: fixed PATH resolution bug (all spec tests were broken)
- sash/src/main.rs: SC2148/SC2030/SC2031 shellcheck excludes, ld -z noexecstack

### Spec gate status

Passing (newly): 5.1, 20.3, 20.4, 20.5, 20.6, 23.3, 23.5, 24.10, 24.11,
  24.16, 31.8, 31.9, 31.11, 39.1a, 39.1b, 53.1, 53.2, 53.3, 55.5, 55.6

Still failing: 31.4a/b/c, 55.4, 55.9, 55.11, 55.12, 55.17c, 55.19,
  58.1-58.8 (new gates, no implementation yet)

### Next priorities

1. DD#58 implementation (sexpr parsing in arithmetic, $(f) type error)
2. DD#55 suppress-prefix (gates 55.4, 55.9, 55.11)
3. Push and update TIMELINE
4. Second autonomous batch run with remaining tasks
