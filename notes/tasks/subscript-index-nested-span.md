---
title: "Exprs nested inside a subscript index carry sub-scanner spans"
status: done
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

## Resolution (2026-10-24, batch-531 port step F13, journal 532)

Option 2, generalised. `parse_subscript_index` was not the only re-parse: a
fresh `Scanner` also re-lexed every double-quoted command substitution
(`command_sub_to_expr`) and every sed RE/SUB (`parse_interpolating_word`), so
a statement inside `"$(…)"` on line 3 reported `1:1` too. It was a family, so
the fix is in the scanner rather than in one caller.

`lexer::Origin` says where a substring's first byte sits in the program:

- `At(line, col)` — the text is a **verbatim** copy of the source, so a
  sub-scanner built with `Scanner::with_origin` numbers from there and every
  span it produces is exact. Every lexer producer copies verbatim, so this is
  the normal case. `Origin::after(prefix)` advances past a known prefix, which
  is how each re-parse finds its own start (the index after `name[`, a sed
  edit after `:s/` and the edits before it).
- `Within(line, col)` — the text was **rebuilt**, not copied (arithmetic
  re-serialises its tokens), so its byte offsets mean nothing in the source.
  Everything inside reports the enclosing construct's position. It is sticky
  through nesting, so a rebuilt text can never be mistaken for a verbatim one.

The `${` / brace-less opener width is read from the source byte rather than
assumed, which was this file's stated obstacle. DD#114's top-level re-anchor
is kept: the index expression itself still reports at `${name[…]}`, where the
read is, and only what is nested inside it is now exact.

The re-lex sites also dropped their sub-scanner's fatal diagnostics; see the
DD#115 addendum.

Gate, measured on the base (`1309385`) and ported binaries. A capture nested
in an index on line 3, `echo "${m[$(nosuchcmd)]}"`, drew SB5011 at `1:3`; it
now reports `3:13`. The same holds unquoted (`3:12`), as an assignment's value
(`3:9`), in a concat index `${m[a$(nosuchcmd)]}` (`3:14`), and for a read in
the capture, `${m[$(echo "$nope")]}` (SB5031, `1:8` → `3:18`). Arithmetic
behaves as the DD#114 addendum says. The gate's own example,
`${a[$((j+1))]}` with `j` unassigned, reports SB5031 at the enclosing `${`
(`3:6`) on both binaries, because that arithmetic is the index expression and
keeps the top-level re-anchor. One level deeper, `${m[$(echo $((j+1)))]}`
reports the `$((` at `3:18` (base `1:8`). Both are real source positions.
