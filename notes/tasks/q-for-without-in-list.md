---
title: "Question: a `for` with no `in` list compiles and never runs, and `for x; in …` is accepted"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG rules on Q1 and Q2; each ruling is recorded in DD#68, whose Forms list `for NAME in LIST { … }`"
  - "every program in the tables runs as its ruling says or is rejected with a diagnostic naming the rule, in both backends"
  - "whatever the ruling, sash-convert writes bash's `for x; do`, `for x do` and `for x` + newline `do` with the explicit list: `for x in @args` in a function, `for x in $(argv 1)` at top level"
---

## What

Found 2026-10-27, while porting F1+F4 (journal 537). `parse_for`
(`sash/src/parse/parser.rs`) is untouched by the port, and every result below
is the same at `8e28c61` and with F1+F4, in both backends.

### Q1. A `for` with no `in` list

`parse_for` treats the `in` list as optional, and a missing list is an empty
one, so the loop never runs. DD#68's Forms list only `for NAME in LIST { … }`,
and no DD says what the shorter form means. In bash it iterates `"$@"`.
DD#75 §5 removed `$@` and spells that iteration `for x in @args` in a
function and `for x in $(argv 1)` at top level (DD#60's argv[1:] splat).

| Id | Program, run with arguments `p q` | sash | bash |
|---|---|---|---|
| g7 | `for x; do echo "$x"; done` | prints nothing, rc 0 | `p`, `q` |
| g9 | `for x`, newline, `do echo "$x"; done` | prints nothing, rc 0 | `p`, `q` |
| g11 | `for y do echo "[$y]"; done` | prints nothing, rc 0 | `[p]`, `[q]` |
| g10 | `for x { echo "$x" }` | prints nothing, rc 0 | not bash |
| f1 | `fn f { for x { echo "[$x]" } }`, then `f a b` | SB5043 4:1 "function 'f' takes exactly 0 arguments but was called with 2" | not bash |
| g12 | `for x in; do echo "[$x]"; done`, then `echo end` | `end` | `end` |

g12, an explicit empty list, agrees with bash. In a function the arity check
catches f1, but its message blames the call, not the loop.

sash-convert passes `for x; do` through unchanged and adds no MANUAL note
(f2). A converted bash loop over the arguments therefore does nothing at top
level, and in a function it draws f1's SB5043. `for y do` gets "MANUAL: could
not parse `for y`". The explicit `for y in "$@"` converts correctly: it
becomes `@args` in a function and `$(argv 1)` at top level (f3, f4 print
`[p]`, `[q]`).

### Q2. A `;` before `in`

`parse_for` skips any `;` or newline between the variable and `in`
(`skip_terminators`).

| Id | Program | sash | bash |
|---|---|---|---|
| g5 | `for x; in a b; do echo "$x"; done` | `a`, `b` | "syntax error near unexpected token `in'" |
| g6 | `for x`, newline, `in a b; do echo "$x"; done` | `a`, `b` | `a`, `b` |

Bash's grammar takes a newline before `in` but not a `;`, because there
`for x;` is the no-list form of Q1.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Options

- **Q1 (a) reject a `for` with no `in` list**: SB4014 at the variable,
  advising `for x in @args` in a function or `for x in $(argv 1)` at top
  level. This keeps DD#75 §5's removal of the implicit `$@`.
  Recommended.
- **Q1 (b) give it bash's meaning**: iterate `@args` in a function and
  `$(argv 1)` at top level. That is an implicit splat, the thing DD#75 §5
  removed `$@` to avoid.
- **Q1 (c) keep the empty list**, and record it in DD#68. The loop body is
  then code that compiles and can never run.
- **Q2 follows Q1.** Under (b), `for x;` means "the arguments", so
  `for x; in a b` is ambiguous and should be rejected as bash rejects it.
  Under (a) or (c) it is unambiguous and can stay, recorded in DD#68.
  Recommended: keep it, with Q1 (a).

Probes: `files/port-f1f4-probes.tar.gz`, directory `qq/` (g5-g7, g9-g12,
f1-f4).
