---
title: "Question: should the attest shorthand on an int (`attest n -n`, `attest n i?`) stay SB4030, as D1 makes it?"
status: open
priority: P4
kind: question
depends: []
gates:
  - "DG rules on the question below; the ruling is recorded as an addendum to DD#93 (Part B)"
  - "the ruling is pinned by specs in both backends: `attest n -n` and `attest n i?` on an int"
---

## What

DD#93 Part B makes `attest VAR OP` sugar for `attest VAR [[ OP $VAR ]]`, for
any unary test operator (`attest x -n`, and Part C's `attest x i?`). The
operand it writes is unquoted. D1 (journal 535; DD#30's addendum of
2026-10-25) makes a whole-int operand of an attest predicate SB4030. So on an
int the shorthand is now rejected, and the message says to write it out in
full: "(a shorthand such as `attest n -n` is written out in full:
`attest n [[ -n "$n" ]]`)".

| Id | Program | HEAD before D1 (bf48bec) | D1 |
|---|---|---|---|
| c1 | `int n=5` / `attest n -n` / `echo "$n"` | prints `5` | SB4030 "int value in an `attest` predicate, which tests strings — quote it …" |
| q1 | `int n=5` / `attest n i?` / `echo "got $n"` | prints `got 5` | SB4030, the same |
| k1 | `int n=5` / `attest n [[ -n "$n" ]]` / `echo "got $n"` | prints `got 5` | prints `got 5` |
| q4 | `int n=5` / `attest n [[ i? "$n" ]]` / `echo "got $n"` | prints `got 5` | prints `got 5` |

The attest-all form is unaffected. An element binder is not typed as an int,
even over an int array, just as a `for` variable is not:
`int array a=(1 2)` / `for e in @a; do echo $(( e + 1 )); done` is SB4030
"bare string `e` in arithmetic". So `attest e in a i?` and
`attest e in a [[ $e == 1 || $e == 2 ]]` compile and run, before and after D1.

The question has little weight today. An attest on an int only matters if the
int can be tainted. The one measured way to taint an int, `readline` into
one, crashes the compiler (`bug-readline-into-int-ice`). That task recommends
SB4030 for it, after which no measured channel taints an int.

## Open question

- **(a) Keep it as D1 landed it.** The shorthand is sugar, and its expansion
  has an unquoted int, so it is SB4030 like the written-out form. On an int,
  `-n` is always true, `-z` always false and `i?` always true; a range check
  is `(( ))`.
- **(b) Quote the operand in the desugaring** (`[[ OP "$VAR" ]]`), so the
  shorthand is legal on any scalar. The sugar would then differ from the
  written-out form that DD#93 Part B defines it to equal.

Recommendation (made during D1, not yet ruled): (a).

Found during the batch-531 port (step D1, when its rule reached the attest
shorthand). Filed under DG's rule for the port that questions and uncovered
cases become backlog tasks.
