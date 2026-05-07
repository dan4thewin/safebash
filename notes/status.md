---
created: 2026-05-07
modified: 2026-05-07
title: Status
---

## Current Priority

Presentation prep + compiler tasks.

## Branch: split-converter (43 commits, pushed)

### This session accomplished:
- Regex engine: added \d \w \s and negations to Plan 9 regcomp.c (28 lines)
- Shorthand classes work standalone and inside [] brackets
- 126 regex tests in sash-rt/csrc/test_regex.c, wired into `mk test`
- Fixed janet heredoc newline bug (raw newlines → \n escapes)
- Converter: rewrites POSIX [-a] to Plan 9 [\-a] in regex patterns
- known_externals.rs: patterns now use \w \d \- (raw strings)
- mkfile: force sash-rt rebuild when C sources change
- Removed dead code from sash-rt (~200 lines: match_bracket, sash_arith_eval, sash_streq, sash_shell_quote, sash_expand_escapes, fnmatch ? and [] arms)
- Removed auto-bless from compiler (~600 lines, known_externals.rs deleted from sash)
- Removed magic variables from SSA taint (no special vars, all via builtins)
- DD#55 implicit exit checks + pipefail + command substitution exit check
- DD#31 reject mutations in condition (()), prefix ++/--, pure comparison
- DD#22/28 IFS rejected, for-in expansion rejected
- DD#23 command resolution (absolute paths required)
- DD#20 reject globstar **
- EXIT trap scope fix (bug #1)
- DD#56 redesign: END block + signal builtin + numeric args
- DD#55 $(status) first-expression rule, $?/$! sugar
- Autonomous task runner (run-compiler-tasks.sh) — ran 3 tasks, all failed due to agent regressions
- New task: improve-agent-prompt.md (fix prompt to enable autonomous work)

### Test stats at session end:
- 162 passing / 86 failing spec tests (was 67/161 at session start)
- 590+ internal tests passing (oracle, codegen, e2e, janet, taint)

### Presentation:
- ~/work/lsg-talk/safebash-pitch.md — 9-slide pitch deck (markdown)
- ~/work/lsg-talk/pitch2pptx.py — converter (handles **bold**, `code`, font sizing)
- Needs: python-pptx + lxml installed to generate .pptx

### Next priorities:
1. Generate the .pptx deck (install python-pptx)
2. Implement improve-agent-prompt.md and re-run autonomous loop
3. Continue compiler tasks: dd55-suppress-prefix, dd56-signum-keyword, dd31-enforce-disjoint
4. Plan 9 regex: add counted repetition {N,M} (regex-counted-repetition task)

### Key decision this session:
- Plan 9 regex is the standard — \- required inside [], no {N} repetition yet
- Auto-bless is converter-only; compiler has zero auto-bless
- Zero magic variables in compiler; all accessed via builtins ($(status), $(getpid), etc.)
- Signal model: END block + `signal N DISPOSITION` (numeric args, GPP for names)
