---
title: "Bug: a `[[ … { } ]]` dispatch is accepted as an `elif` condition and as a `&&`/`||` right operand"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "a dispatch as an `elif` condition (brace and keyword spellings) is SB4028, at the dispatch's position"
  - "a dispatch as the right operand of `&&` or `||`, as a statement or inside an `if`/`while` condition, is SB4028"
  - "one predicate decides every position DD#66 names; no boolean position is checked by a private test"
  - "specs pin each position in both backends; each fails on the pre-fix binary"
---

## What

DD#66 (2026-06-19, "Composition: statement-only") says a dispatch
`[[ … ]]` (groups present) may not be "the condition of `if`/`while`/
`until`, … the operand of `!`, or … an operand of `&&`/`||`. Otherwise its
always-`0` status would make such chains silently always-succeed."

The parser enforces this in three places only. Probed 2026-10-24 on base
`1309385` and on the F13 worktree; both binaries behave the same, in both
backends:

| probe | program (inside `fn f { … }; f a`) | result |
|---|---|---|
| d3 | `if [[ $1 == a { echo y } ]] { echo b }` | SB4028 (correct) |
| o4 | `! [[ $1 == a { echo y } ]]` | SB4028 (correct) |
| d2 | `if /bin/false { echo a } elif [[ $1 == a { echo y } ]] { echo b }` | compiles, prints `y b` |
| d4 | the same with `then … elif … then … fi` | compiles, prints `y b` |
| o2 | `/bin/true && [[ $1 == a { echo y } ]]` | compiles, prints `y` |
| o5 | `/bin/false \|\| [[ $1 == a { echo y } ]]` | compiles, prints `y` |
| o6 | `if /bin/true && [[ $1 == a { echo y } ]] { echo b }` | compiles, prints `y b` |
| o7 | `if /bin/true && [[ $1 == z { echo y } ]] { echo b }` | compiles, prints `b` |
| o1 | `[[ $1 == a { echo y } ]] && echo z` | SB4023, a compound on `&&` (rejected, by another rule) |

o7 is the footgun DD#66 names: no arm matches, the condition succeeds
anyway, and the `if` body runs.

## Cause

`stmt_is_dispatch` (`parse/parser.rs`) tests one statement's own kind.
Three callers apply it: `parse_if` and `parse_while` to each top-level
statement of the condition list, and `parse_negated` to its operand.

- `parse_brace_elif` and the elif loop in `parse_if_keyword` never call it.
- `parse_trailing_list_op` takes its right operand from
  `parse_list_operand` and never calls it. Because the `if`/`while` checks
  look only at top-level statements, a dispatch nested in an `&&`/`||`
  list inside a condition (o6, o7) escapes those checks as well.

The left operand is rejected already: a dispatch parses as a compound, and
`compound_postfix_rejection` refuses `&&` after a compound (SB4023).

## Fix

Check the elif conditions in both spellings, and the right operand in
`parse_trailing_list_op`. The second also covers o6/o7, because the
nested dispatch is then rejected where the list is built. Whether the left
operand should say SB4028 instead of SB4023 is a wording choice; both
reject it.

Probes: `files/port-f13-probes.tar.gz`, directory `el2/` (d2–d4, o1–o7).
