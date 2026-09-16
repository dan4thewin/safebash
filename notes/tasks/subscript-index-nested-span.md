---
title: "Exprs nested inside a subscript index carry sub-scanner spans"
status: open
priority: P3
kind: diagnostic
depends: []
gates:
  - "a diagnostic on an expression nested inside a subscript index (e.g. the `$j` in `${a[$((j+1))]}`) reports the real source line:col, not one relative to the index text"
---

## What

`parse/expr.rs::parse_subscript_index` re-parses the captured index text with
a fresh `Scanner`. `Span` is `{line, col}` (not a byte offset), so that scanner
numbers from the start of the index *substring*: every expression it builds
carries a span relative to that text, and a single-line index yields `1:1`.

DD#114 re-anchors the **top-level** index expression to the span of the
enclosing `${name[…]}`, which fixes every diagnostic that reports on the index
itself (SB5020 taint reads, SB4041 ambiguous index). Expressions nested deeper
— inside an arithmetic index, a command substitution used as an index, a
quoted concat — still carry the relative span.

## Why it is P3

To hit it you need a diagnostic that fires on a sub-expression of an index. In
practice indexes are a bare name, a literal, or a short arithmetic expression,
and the common diagnostics all report on the index expression itself.

## Options

1. **Deep re-span.** Walk the parsed index and overwrite every span with the
   enclosing one. Correct, never misleading, but flattens real structure — and
   the walk has to cover `CommandSubstitution` (statements) and `Test`, which
   means statement and test walkers as well.
2. **Give `Scanner` a base position.** `Scanner::new_at(text, line, col)`,
   with every span offset by it (col offset applies only on the first line).
   Genuinely correct spans everywhere. The caller must compute the index text's
   real start, which `try_subscript` can do from `span` and `name_end` but only
   by assuming the `${` prefix.
3. **Leave it.** Accept relative spans on nested index sub-expressions.

Option 2 is the real fix and would also help any other place that re-parses a
captured substring. Worth checking whether there are others before doing it —
if `parse_subscript_index` is the only one, option 1 is cheaper.
