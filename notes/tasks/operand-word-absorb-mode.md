---
title: "Operand-word lexer mode: `=~`/`==` RHS + assignment values absorb to whitespace; delete parse_regex_rhs; multi-exact dispatch arms"
status: done
priority: P1
kind: feature
depends: []
gates:
  - "`=~` RHS parses as an operand word: bare metachars work (`^(a|b)$`), a quoted space works (`[[ $x =~ 'a b' ]]` matches `a b`), and `[[ $x =~ foo bar ]]` is NOT silent `foobar` (dangling `bar` → clean error)"
  - "`parse_regex_rhs`/`regex_rhs_step`/`regex_punct_char` deleted; the `{` whitespace-gap heuristic gone"
  - "assignment/declaration RHS absorbs metachars to `;`/newline/whitespace: `a=foo|cat` → literal `foo|cat`; `a=foo bar` → `a=foo` then `bar` was the original gate (retired by DD71's 2026-11-09 boundary policy); `a='x y'` → `x y`"
  - "multi-exact dispatch arm `[[ $x == a b c { … } ]]` runs the group for a|b|c (first-match); backends unchanged (CaseItem.patterns already Vec)"
  - "dynamic `=~ \"$x\"` supported (validate-static / runtime-abort-dynamic); native+c+janet identical; 0 regressions"
---

# Operand-word lexer mode (DD#71)

Implements the DD#71 operand-word model. Supersedes
`bug-regex-rhs-space-quote` (resolved by construction) and enables the
multi-exact dispatch arms discussed alongside DD#66.

The original whitespace-only assignment-to-command split is historical,
not current language policy. DG retired it on 2026-11-09: DD71 now
requires a separator after one or more bindings before a following
statement, while preserving actual whole-statement boundaries.
`q-statement-form-residuals` owns enforcement; this completed lexer
task is not reopened, and its operand-word extent rule stays unchanged.

## Scope

1. **Operand-word lexer mode** — a word that absorbs non-whitespace chars
   (regex/shell metachars literal) up to the delimiter, honouring
   `$`/`` ` ``/quotes as sub-structure starters. Delimiters:
   - `[[ ]]` operands / dispatch patterns → unquoted whitespace or `]]`
     (whitespace before an arm `{` ends the pattern);
   - assignment / declaration RHS → unquoted whitespace, `;`, or newline.
   Command-arg position is unchanged (operators stay operators).
2. **`=~` and `==` RHS parse through the same operand-word routine.** Delete
   `parse_regex_rhs`/`regex_rhs_step`/`regex_punct_char` and the `{`
   whitespace-gap heuristic (now the natural rule: a bare `a{2,3}` is one
   word; a space-separated `{` is an arm).
3. **Multi-exact dispatch arms** — `parse_dispatch` collects multiple
   space-separated patterns per arm into `CaseItem.patterns` until the
   arm `{`. Detection: after `SUBJECT == OPERAND`, a bare word (not `{`,
   `]]`, `&&`, `||`) means more patterns follow. Backends already lower
   multi-pattern arms; parser-only. `=~` arms stay single-regex.
4. **Assignment/declaration value = operand word** — `a=foo|cat` etc. are
   literals; whitespace/`;`/newline terminate.

## Watch out

- Only the *operand* slots inside `[[ ]]` are absorb-mode — the boolean
  skeleton (`&& || ! ( )`) must stay operators. The lexer mode is entered
  where the parser expects an operand/pattern, not for the whole `[[ ]]`.
- The lexer is currently context-free (tokenises `( | { $` unconditionally).
  The mode is a parser-requested "lex the next thing as an operand word"
  that reassembles from the source span (quote/`$`-aware), OR a scanner
  sub-mode. Determine the cleanest hook (scanner peek that returns a raw
  word) at implementation.
- A literal space in an operand needs quoting (`'a b'`) — consistent across
  `[[ ]]` and assignments. Add specs asserting the dangling-word error for
  an unquoted space where a single operand is required.
- Dynamic `=~ $pat` / `=~ "$x"`: static → comptime regex-validate; dynamic
  → runtime regcomp-abort (DD#55 A). Do not reject dynamic patterns.

## Spec migration

Existing unquoted-`=~` specs (dispatch.sh, control_flow cf.c5–c8,
wordsplit) keep working (bare metachars unchanged). Add: quoted-space
regex, multi-exact `== a b c`, assignment-literal-metachar, dynamic
pattern. Cross-check the DD#66 dispatch specs still green.

## Refs

- `notes/decisions/DD71-operand-words-value-bindings.md`
- supersedes `bug-regex-rhs-space-quote`
- sibling `bug-dispatch-literal-star-wildcard` (the `*`-literal / `is_default`
  fix — land together so dispatch pattern semantics are consistent)
- `reject-nonstatus-in-boolean-context` (the DD#70 dual)
