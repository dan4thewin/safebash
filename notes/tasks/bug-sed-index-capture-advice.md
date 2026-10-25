---
title: "BUG: a sed-expansion index on an indexed array gets the capture advice, which it cannot follow"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "a `${k:s/…/…/}` index on an indexed array is still SB4030 in every spelling, with advice the author can follow"
  - "a written-out `$( … )` index keeps the capture message"
---

## What

A `${k:s/…/…/}` index on an indexed array is SB4030 in every spelling, as
DD#80 requires, but the message is the one written for a capture:

> capture as an index on indexed array 'a' — `$( … )` yields a string, and an
> int-indexed array is indexed by number (DD#80). Declare the command as an
> int-returning function and call it in arithmetic: `fn f -> int { … }` then
> `${a[$(( (f) ))]}`; declare `map a` for string keys

| Id | Program | Today |
|---|---|---|
| a09 | `array a=(10 20)` / `str k=x` / `echo ${a[${k:s/x/1/}]}` | SB4030 at 3:6, the capture message |
| a10 | the same, spelled `echo $a[${k:s/x/1/}]` | the same |
| a11 | the same, spelled `echo "$a[${k:s/x/1/}]"` | the same |
| ic6 | `array a=(10 20)` / `str k=x` / `a[${k:s/x/1/}]=7` / `echo ${a[1]}` | SB4030 at 3:1, the same |
| c22 | `array a=(10 20)` / `str k=']'` / `a[${k:s/]/1/}]=7` / `echo ${a[1]}` | the same |

Results were measured after F15, and are the same in both backends, on the
base commit 1309385 and on the batch-531 branch.

The author wrote no command, so there is no function to declare. DD#80's
2026-10-03 addendum ("Why the capture gets its own diagnostic") gives the
capture this message because "an author holding a string-returning function
has exactly one migration". It cites `array_index_error`'s standing rule
that a diagnostic must not send its reader from one SB4030 to the same
SB4030. A sed index has a different migration, and the message does not
name it.

The parser desugars the sugar to `$(sub "$k" …)` before type checking, so
`array_index_error` sees a capture.

The migration that works is through a variable. This prints `20`:

`array a=(10 20)` / `str k=x` / `str j=${k:s/x/1/}` / `echo ${a[$(( ${j:?not a number} ))]}`

The inline form `${a[$(( ${k:s/x/1/} ))]}` also prints `20` today, but only
because of the bug in `bug-arith-operator-expansion-unconverted`. That
task's fix makes it SB4030.

## Fix

`array_index_error` recognises the desugared sed (or the desugar marks it)
and gives advice that names the variable. The exact wording is left to the
implementer.

Recommendation (made during F15, not yet ruled): reuse the string-index
advice, with the variable introduced: "a `${k:s/…}` index yields a string,
and an int-indexed array is indexed by number (DD#80). Assign it to a
variable and convert it with a disposition: `str j=${k:s/…}` then
`${a[$(( ${j:?not a number} ))]}`; declare `map a` for string keys".

## Related

Two other tasks lead more programs to this message once they land:

- `bug-subscript-key-check-skips-positions`: with an operator, the program
  compiles today (its ic11).
- `bug-arith-braced-subscript-index-second-grammar`: inside `$(( ))` without
  an operator, the program is SB4041 today (its a12).

Found during the batch-531 port (step F15, beside finding c22 of the sed
probes). Filed under DG's rule for the port that questions and uncovered
cases become backlog tasks.
