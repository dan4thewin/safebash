---
title: Status
modified: 2026-05-24
---

## Current branch: hand-parser

## Spec status: 253 passing / 29 failing (spec.db)

### Session 2026-05-24 (continued) — tree-sitter removal

**Tree-sitter dropped from compile path.** Single parser architecture:
source → hand-written parser → IR → validate → transform → SSA → codegen

**New `validate.rs`** (IR-walking validator, ~530 lines):
- SB5001: export rejected
- SB5017/5018: rejected vars + bare env vars
- SB5019: numeric test operators in [[ ]]
- SB5006: restricted globs (**, ?, [...] in unquoted position)
- SB4001: dynamic execution (eval, source, coproc)
- SB5007: arithmetic mutation in conditions + bare comparisons
- SB5009: IFS assignment
- SB5020: taint tracking (getenv, argv, read, external commands)

**Taint system** — simple forward-pass:
- Track tainted variable names in a HashSet
- Sources: $(getenv), $(argv), read from stdin, /absolute/cmd
- Clean: user function captures, read from herestring/heredoc,
  pipeline-internal reads
- bless removes taint
- $1-$9 at top level implicitly tainted

### Test suite
- spec: 253/282 (was 274 with tree-sitter, dropped to 223, recovered to 253)
- codegen: 185/264 (5 intentional rejections from compound redirects)
- e2e: 97/124
- oracle: 59/59

### Remaining 29 failures
- 7 pre-existing (21.2, 39.2, 39.5, 39.6, 39.7, 55.11 + one more)
- 3 spec tests need rewriting (14.4-14.6: expect rejection now)
- 5 taint edge cases (24.4, 24.10, 24.11, 24.16, 50.1)
- 4 SB5007 edge cases (31.5, 31.6, 31.8, 31.11)
- 5 $? rules (55.5, 55.6, 55.15, 55.17a, 55.17b)
- 3 word-splitting/command-resolution (22.3, 22.11, 23.2, 23.3, 28.1)
- 1 heredoc cmdsub (5.1)

### Architecture
- Tree-sitter still in Cargo.toml (for lower/ module used by oracle_tests,
  and trap handler in transform/cmd_handlers.rs)
- Old validator/taint/rules modules still exist as source files but are
  no longer compiled into the binary (removed from lib.rs mod list)
- validate.rs is the single source of truth for validation
