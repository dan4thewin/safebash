---
title: "BUG — `[[ =~ ]]` regex RHS silently drops spaces and rejects quotes (no way to match a space)"
status: needs-decision
priority: P1
kind: bug
depends: []
gates:
  - "a `=~` regex can contain a literal space (per the surface syntax DG chooses) and matches correctly"
  - "`x='foo bar'; [[ $x =~ <space-form> ]]` prints yes; `x=foobar` prints no (no silent concatenation)"
  - "no silent miscompile: an unrepresentable/ambiguous space form is a clean compile error, never a wrong match"
  - "native + c + janet identical; dispatch `=~` arms consistent with boolean `=~`"
---

# BUG — `=~` boolean RHS mangles whitespace and drops quotes

Found 2026-07-04 (DG asked "what if the pattern contains a space followed
by `]]`?"). Two defects in the same code path.

## Repro

```
$ x='foo bar'; sash -c '[[ $x =~ foo bar ]] && echo yes || echo no'    # -> no   (WRONG, want yes)
$ x=foobar;    sash -c '[[ $x =~ foo bar ]] && echo yes || echo no'    # -> yes  (WRONG, want no)
$ x='foo bar'; sash -c "[[ \$x =~ 'foo bar' ]] && echo yes || echo no" # -> SB4014 rejected
```

1. **Silent space-drop (miscompile).** `foo bar` is compiled to the regex
   `foobar` — inter-token whitespace is deleted with no error, so the
   pattern silently means something else. Worst kind of bug.
2. **Quotes rejected.** A quoted regex (`'foo bar'` / `"foo bar"`) collapses
   to empty and errors SB4014, so the obvious escape-hatch for a space
   doesn't exist.

Net: a boolean `=~` regex **cannot represent a literal space** today.

## Root cause

`regex_rhs_step` (`sash/src/parse/parser.rs`, ~1222):

- Pushes each `Tok::Word` with **no separating whitespace**
  (`pattern.push_str(w)`), so `foo`,`bar` → `foobar`.
- Handles only `Word`/`Dollar`/punctuation (`regex_punct_char`). A
  `SingleQuoted`/`DoubleQuoted` token matches neither, contributes nothing,
  and the accumulated pattern ends empty → `parse_regex_rhs` raises SB4014.

The dispatch `=~` first-arm pattern flows through the same `parse_regex_rhs`
path, so it shares the defect; check consistency with later dispatch arms
(which currently parse via `parse_expr`).

## DECISION NEEDED — how does a `=~` regex express a literal space?

The silent space-drop is a bug to fix unconditionally; the *surface syntax*
for an intended space is a design call (DD#62/#63 family — quotes are
grouping, not literalization). Options discussed:

- **(a) quoting** — `=~ 'foo bar'` / `"foo bar"`: the quoted run is emitted
  verbatim (spaces preserved), quotes are grouping. Matches DD#62's stance
  and bash muscle-memory. Needs `regex_rhs_step` to handle quote tokens.
- **(b) backslash-escape** — `=~ foo\ bar`.
- **(c) preserve source whitespace** between tokens up to `]]` — couples the
  pattern to lexer spacing; brittle.

Whatever is chosen, an unrepresentable form must be a **clean compile
error**, never a silent concatenation.

## Relationship

Sibling of `bug-dispatch-literal-star-wildcard` (same session). Related to
`regex-interpolation-delimiters` (DD#63 remainder) and the pre-existing
"quoted-regex-in-`=~`" gap. Blocks nothing in the multi-exact dispatch work
(that is `==`-only) but should be resolved before leaning harder on `=~`.
