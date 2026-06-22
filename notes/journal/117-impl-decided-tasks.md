---
title: 117 — implementing the reviewed decisions (2026-06-22)
---

# Implementing the decided tasks

After the 2026-06-21 decision review (5 decisions resolved with DG), DG
directed implementation with the recent SOP: dependency order, file
decisions/non-blocking-bugs as tasks, fix blocking bugs inline, journal
commentary, no scope reduction, switch when done/blocked.

Dependency order: advance-dd23-resolution → tighten-cmdsub-taint-execve;
regex-builtins → vet-var-modifiers; glob-splat Part 2 (shared word-interp
with vet); reject-undeclared / dd67-sugar / getpid independent.

## advance-dd23-resolution — starting

Hoist command resolution to a front-end pass (before validate/taint/
type-check) so the front-end sees absolute paths. Unresolved → fatal.
Key effect: the CST taint pass already taints `name.starts_with('/')`, so
resolved externals become correctly tainted with a span — fixes the
unblessed-external ICE and realizes Decision 5's taint goal (printf →
/usr/bin/printf → tainted).

## Reorder note

advance-dd23 turns out to be a coordinated cluster: `printf`/`pwd` are
`BuiltinCmd` entries that actually `execve`, so the internal-vs-external
(execve) classification is shared by advance-dd23 + tighten-taint +
getpid. Rather than half-build that, I'll do the clean self-contained
decided tasks first, then tackle the resolution/taint/getpid cluster as a
coordinated set. Starting with **dd67-sugar-sweep** (parser sugar).

## dd67-sugar-sweep — starting

Three ratified changes: re-add `name() {}` (→ fn sugar; currently SB4014);
re-add flat `` `cmd` `` (→ `$(cmd)`); single-bracket `[` becomes a normal
literal character (no `[ … ]` test; `[[ … ]]` only). `function name {}`,
`then…fi`, `do…done` already accepted.

### §1 name() {} — DONE

Added `parse_paren_function` + `is_paren_function` (WORD `(` `)` lookahead)
+ a dispatch guard; extracted `finish_function_def` shared with
`parse_function`. `name() { }` and `name() -> int { }` produce the same
`Function` IR as `fn name { }`. Specs functions 67.1-67.3 (all backends).
Big win: rust suite 180→152 — `name() {}` was unblocking ~34 tests.
