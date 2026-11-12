title: "Bug: an index with an unclosed quote, parenthesis or `]` is reported as an unrecognised parameter operator (SB4047)"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "no program in the first table is reported as an unrecognised parameter operator; each gets a diagnostic for the index's real fault, in both backends"
  - "the rows whose index opens with `\"`, `'` or `$(` report that form's own error (DD#80's addendum of 2026-10-24)"
  - "`${a[$(( 1 + ))]}` keeps SB4020 at the index (B39, DD#80's addendum of 2026-10-24)"
  - "an operator that really is unrecognised keeps SB4047: `${x&y}`, `${x:foo}`, and every existing SB4047 spec"
  - "a spec pins each opener in both backends; each fails on the pre-fix binary"

## What

With `array a=(x y)` on line 1, each of these is rejected in both backends,
on the base binary (1309385) and after F14. (A spec needs the `array`
keyword: `a=(x y)` binds a scalar, which draws SB5068 once the index
parses.)

| Line 2 | Diagnostic |
|---|---|
| `echo "${a["x]}"` | SB4047 at 2:6, "unrecognized content `["x]` after `a` in `${…}` — the compiler does not recognise this parameter operator and will not silently ignore it. sash's parameter operators are `:-` / `:?` / `:off:len` / `:s/…/…/` (plus the string builtins toupper/tolower, sub/gsub, basename/dirname)" |
| `echo "${a['x]}"` | SB4047 at 2:6, "unrecognized content `['x]` …" |
| `echo "${a[$(echo]}"` | SB4047 at 2:6, "unrecognized content `[$(echo]` …" |
| `echo "${a[(1+]}"` | SB4047 at 2:6, "unrecognized content `[(1+]` …" |
| `echo "${a[1}"` | SB4047 at 2:6, "unrecognized content `[1` …" |
| `echo ${a["x]}`, `echo ${a[$(echo]}`, `echo ${a[(1+]}`, `echo ${a[1}` | SB4047 at 2:6 |

The rejection is right, but the diagnostic misdirects. It calls the index a
parameter operator and lists the operators, when the fault is an unclosed
`"`, `'`, `(` or `$(` inside the index, or a missing `]`. In the quoted
rows, 2:6 is the opening `"`; the `$` is at 2:7.

The arithmetic spelling is already right: `echo "${a[$(( 1 + ))]}"` is
SB4020 at 2:11, "arithmetic expression ends where an operand is expected".
That changed with B39 in F14; base said SB4041 at 2:6.

No spec covers these shapes.

## Cause

`try_subscript` (`sash/src/parse/expr.rs`) captures the index with
`capture_balanced_subscript`, which balances `[] {} ()` and skips quoted
text (DD#80). An unclosed `"`, `'`, `(` or `$(` in the index swallows the
closing `]`; so does a missing `]`. The match fails, and `try_subscript`
returns `Ok(None)`, as if the expansion had no subscript at all.
`parse_parameter_expansion` then tries the text after the name as an
operator. `parse_cell_operator` declines it, and
`unrecognized_parameter_operator` builds the sentinel that
`check_rejected_param_expansion` (`sash/src/parse/parser.rs`) reports as
SB4047.

This is the same shape as B39 (DD#80's addendum of 2026-10-24): a fallback
that hides the index's own error.

## Fix (proposed, not yet ruled)

Once `try_subscript` has seen the `[`, a failed match is the index's error,
not "no subscript". That needs `capture_balanced_subscript`, or a sibling,
to say where the match failed. What to report depends on the form:

- **An index that opens with `"`, `'` or `$(`** is an explicit form.
  DD#80's addendum of 2026-10-24 says an explicit index that fails to parse
  reports its own error. That addendum was written for an index that can be
  delimited and then fails to parse. These indexes cannot even be delimited,
  but the principle is the same one step earlier: report the unterminated
  quote or `$(` at its own position.
- **An index that opens with `(` or a bareword** is an implicit form, and
  here the addendum leaves a choice. Its third bullet keeps an implicit
  index that fails to parse at SB4041 ("make the index explicit"). An
  unclosed `(` or a missing `]` could follow that bullet, or could instead
  name the unclosed `(` or the missing `]`. Open question: which.

## Related open question

A `{` inside the index, or an inner `${` that the index never closes, gets a
different diagnostic, SB4057, "unterminated `${` — it is never closed before
the end of the source, so everything after it was read as part of it":

| Line 2 | Diagnostic |
|---|---|
| `echo "${a[{1]}"` | SB4057 at 2:7 |
| `echo ${a[{1]}` | SB4057 at 2:6 |
| `echo "${a[${i]}"` | SB4057 at 2:7 |

This one is probably as designed. The lexer finds the expansion's extent by
brace depth before the parser sees the index (`Scanner::scan_braced_content`
→ `scan_by_brace_depth`, `sash/src/parse/lexer.rs`), and DD#65 makes the
first unnested `}` the close. When no `}` is left for the outer `${`, the
lexer records it as unterminated and `scanner_fatal`
(`sash/src/parse/parser.rs`) reports SB4057 at the outer `${`. The message
says what happened, but not where the stray `{` is. The fix above cannot
reach these rows, because the lexer has already taken the `}`. Whether they
should name the `{` is an open question.

Found during the batch-531 port (step F14, while probing finding B39).
Filed under DG's rule for the port that questions and uncovered cases
become backlog tasks.
