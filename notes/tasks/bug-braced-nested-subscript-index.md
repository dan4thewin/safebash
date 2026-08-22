---
title: "bug: braced ${a[a[0]]} nested subscript index reads the inner cell, not the outer"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "`${a[a[0]]}` evaluates the index `a[0]` then reads that outer cell (bash parity), instead of silently returning the inner cell value"
  - "no regression"
---

## Context

Found 2026-08-21 while fixing `arith-bare-subscript-followups` edge 2 (the
**bare** nested form `$(( a[a[0]] ))`, now fixed via lexer subscript-depth
tracking). The **braced** form is a separate, pre-existing bug on a different
path.

`int array a=(2 0 9); echo $(( ${a[a[0]]} ))` prints **2** (the inner `a[0]`),
not **9** (`a[0]=2`, then `a[2]=9`). Bash evaluates a subscript in arithmetic
context, so the correct result is the outer lookup. sash silently returns the
inner cell — a silent wrong answer.

## Path (why it's independent of the edge-2 fix)

The braced `${…}` is scanned by `lexer.rs::scan_brace_expansion` (raw content
to `}`), then the subscript index is parsed by
`parse::expr::subscript_index_expr`. That path does not recursively evaluate a
nested `a[0]` as an arithmetic subscript, so `${a[a[0]]}` collapses to the inner
read. The edge-2 fix only touched `scan_word_chars` (bare words), so this
predates it.

## Fix sketch (verify first)

Make `subscript_index_expr` (or its lowering) treat the index as a full
arithmetic expression, so a nested `a[0]` cell read inside `${a[…]}` is
evaluated like the bare `a[a[0]]` form. Confirm parity with bare nesting
(`nsx.1-3` in spec/arithmetic.sh) and add a braced-nested spec. Watch DD#77:
a *string*-array inner cell index needs a disposition (bare `a[0]` string → the
SB4030 gate), so the braced-nested rules should mirror the bare-nested ones.

## Provenance

`notes/tasks/arith-bare-subscript-followups.md` edge 2, journal (edge-1/2
entry).
