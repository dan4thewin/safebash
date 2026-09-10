---
title: DD#71 Cluster A implemented — operand words, value bindings, dispatch
date: 2026-07-04
---

# Cluster A — DD#71 operand-words, spec-first, 5 commits

DG picked "Cluster A — DD#71 operand-words, spec-first" as the next batch.
Shipped all five coupled components, one commit each, **0 regressions
native/c/janet** on every one (authoritative spec.db diff). Native passing
595 → 635.

## The five commits

1. **star-wildcard** (`9424d66`, `bug-dispatch-literal-star-wildcard`) —
   `*` in a dispatch arm is now a literal exact pattern, not a catch-all.
   `CaseItem` gained an `is_default` flag; the `|| { }` arm sets it (no more
   synthetic `*` sentinel), and lowering keys the unconditional-else off
   `is_default` (`is_wildcard_arm` → `is_default_arm`). janet's `_` catch-all
   too. spec/dispatch.sh +dp.e1-e4.

2. **operand-word** (`60c667b`, the core) — the `=~` RHS parses through a new
   `parse_operand_word` (`parse_expr` + absorb `( ) { } | !` as literal chars,
   to the next unquoted whitespace/`]]`), replacing the bespoke
   `parse_regex_rhs`/`regex_rhs_step`/`regex_punct_char` collector and its `{`
   whitespace-gap heuristic. Three bugs fixed *by construction*: quoted spaces
   work (`=~ 'a b'`), an unquoted space is a clean SB4014 (not the silent
   `foobar`), and a **dynamic** pattern (`=~ $pat`) interpolates —
   `test_to_ssa`/`dispatch_test` lower a static `=~` RHS to `Pattern`
   (comptime-validated) and a dynamic one to `Value` (runtime regcomp).
   `check_regex_literal` was rewritten to extract a static pattern from any
   static operand shape (Literal/quoted/Concat) — else multi-token patterns
   like `(?=h)ello` would skip comptime validation (that was the one
   regression the diff caught, janet:39.8, fixed before commit). Quoted
   invalid regexes are now caught at compile time too. spec/operand_words.sh.

3. **multi-exact** (`e9fc520`) — a `==` dispatch arm may carry multiple
   space-separated exact patterns (`[[ $x == a b c { … } ]]`, first-match),
   the OR-of-exacts `==` structurally lacks without re-introducing `|`.
   `dispatch_multi_follows` (a bare word after a `==` binary) enters dispatch;
   `collect_arm_patterns` gathers operand-words until the arm `{`. `=~` arms
   stay single-regex. Parser-only — backends already lower multi-pattern.
   spec/dispatch.sh +dp.f1-f6.

4. **boolean-reject** (`1c2e74f`, `reject-nonstatus-in-boolean-context`) — the
   DD#70 dual: a boolean context (if/elif/while/until cond, left of `&&`/`||`,
   operand of `!`) must consume a boolean/exit-status, so an assignment or
   declaration there is **SB4025**. Fixes the silent `x=hi && echo $x`.
   `validate_nested`/`validate_children` handle the wrapper recursion; the
   left of `&&`/`||` and the `!` operand become boolean contexts.
   spec/errors.sh +e.br1-10.

5. **assign-absorb** (`000dd5a`) — an assignment is a value binding, not a
   connectable command, so its RHS absorbs `& && || |` (and operand puncts)
   as literals: `a=foo|cat` is the literal `foo|cat`, `a=foo&&bar` is
   `foo&&bar`. `parse_word_parts` gained an `AbsorbMode` enum
   (None/Operand/Assign) + `absorb_text` map; `parse_assignment_value` wired
   into the assignment/declaration/`+=` sites; `=(…)` array literals
   unaffected. spec/assign_words.sh.

## The glued-vs-spaced pivot

boolean-reject (4) and assign-absorb (5) interlock cleanly on whitespace:
`a=foo && cmd` (spaced) → `&&` is an operator → assignment-in-boolean-context
→ SB4025; `a=foo&&cmd` (glued) → `&&` absorbed → the literal value `foo&&cmd`.
The mandatory space is load-bearing by design, matching the dispatch arm `{`
disambiguation.

## Filed along the way

- `bug-test-operand-concat-splice` (P3) — pre-existing: `[[ $x == run$suf ]]`
  (literal+var splice) doesn't concatenate at runtime; affects `==` too, so
  it's the `expr_to_value`/`Concat` test-operand lowering, not DD#71 parsing.

## Method notes

- Every component was spec-first (new spec/*.sh or additions), then
  implement, then the authoritative spec.db latest-run-per-test diff across
  native/c/janet. The run.sh tally line is misleading (mixes stale c/janet
  rows); the Python diff is the gate.
- The pre-commit complexity gate fired four times (grandfather-on-modify on
  touched functions) — each fixed by extracting a helper (`validate_nested`/
  `validate_children`, `parse_one_decl_assignment`) or flattening with
  let-else (`check_regex_literal`), never bypassed.
