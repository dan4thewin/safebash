title: DD#59 — Logos for structural-operator tokenisation
status: implemented

1. Use a generated Logos DFA for structural operator families and retain
   hand-written scanning for stateful expansions, quoting and word boundaries.
2. Keyword classification has one authority; unreachable duplicate keyword
   definitions are rivals, not guards (DD116).
3. Doubled test brackets tokenize at adjacent word boundaries; single brackets
   retain their subscript role.

---

The lexer dispatches the first byte of every token. The
"structural" subset — newline, `#`-comment, semicolon family
(`;`, `;;`, `;&`, `;;&`), pipe family (`|`, `||`), ampersand
family (`&`, `&&`, `&>`), parens (`(`, `((`, `)`, `))`), braces
(`{`, `}`), and greater-than family (`>`, `>>`, `>&`) — is
recognised by a `logos`-generated DFA defined in
`sash/src/parse/logos_tokens.rs`. Everything else stays
hand-rolled.

## Boundary

| Hand-rolled                              | Logos                |
|------------------------------------------|----------------------|
| `$…` (variable / expansion forms)        | `\n`                 |
| `'…'` single-quoted                      | `#…` comment         |
| `"…"` interpolated                       | `;` `;;` `;&` `;;&`  |
| `` `…` `` (rejected, span capture)       | `\|` `\|\|`          |
| `<` family incl. heredoc sentinel        | `&` `&&` `&>`        |
| `-` (Dash vs `-flag` word)               | `(` `((`             |
| `!` / `!=`                               | `)` `))`             |
| `=` / `==` / `=~`                        | `{` `}`              |
| `+=` / `+` (word char)                   | `>` `>>` `>&`        |
| `[` / `[[`, `]` / `]]`                   |                      |
| word scanning + keyword promotion        |                      |
| ~~`<<<` HereString (handled in `scan_lt`)~~ | DD83 retires here-strings |

## Why this split

Logos is good at: longest-match operator families, comments,
whitespace skip rules. It compiles to a static DFA that is fast
and exhaustive over the alternatives it knows about.

Logos is bad at: state-carrying scanners (string/expansion
nesting, quote escapes, ANSI-C decode, heredoc delimiter capture),
contextual disambiguation (`-` standalone vs `-flag` prefix,
bare `[` as word char vs `[[` operator), and word-boundary
semantics tangled with keyword promotion. Those stay where they
already work.

~~`<` is hand-rolled (and not delegated) because `<<` is the
heredoc sentinel — it must consume the delimiter word and emit
`Tok::Word("__HEREDOC__")` so the validator can reject heredocs
cleanly. Logos would happily tokenise `<<<` and `<&` and `<>`,
but mixing the heredoc state into a logos rule defeats the
purpose of using a regular-language tool.~~ DD01-09's later scanner rejection
supersedes the heredoc-sentinel route.

## Implementation shape

`Scanner::next_token` (`sash/src/parse/lexer.rs:266`) dispatches
on the first byte:

```rust
match b {
    b'$' => self.scan_dollar(start),
    b'\'' => self.scan_single_quoted(start),
    b'"' => self.scan_double_quoted(start),
    b'`' => self.scan_backtick(start),
    b'<' => self.scan_lt(start),
    b'-' => self.scan_dash_or_word(start),
    b'!' => self.scan_bang_or_neq(start),
    b'=' => self.scan_eq_punct(start),
    b'+' => self.scan_plus_or_word(start),
    b'[' => self.scan_lbracket(start),
    b']' => self.scan_rbracket(start),
    b'\n' | b'#' | b';' | b'|' | b'&'
        | b'(' | b')' | b'{' | b'}' | b'>' => self.scan_via_logos(start),
    _ => self.scan_word(start),
}
```

`scan_via_logos` constructs a `LogosTok::lexer` over the
remaining byte slice (UTF-8 prefix only — comments may contain
UTF-8, in which case the comment text is re-read from the
original `&[u8]` source), takes the first matched token,
advances `self.pos` by the matched span, and rebuilds a
`SpannedToken` via the `to_tok` shim. The `Comment` variant
carries no payload through `to_tok`; the comment string is built
from the byte slice the caller already has.

## Results

| Metric                                  | Before | After |
|-----------------------------------------|--------|-------|
| Cyclomatic complexity of `next_token`   | 78     | 15    |
| Lines in `next_token` + dispatch        | ~440   | ~250  |
| All 285 spec tests passing              | yes    | yes   |
| All cargo lib unit tests passing        | yes    | yes   |
| Clippy with `-D warnings` clean         | yes    | yes   |

The split also adds five new small helpers (`scan_lt`,
`scan_dash_or_word`, `scan_bang_or_neq`, `scan_eq_punct`,
`scan_plus_or_word`, `scan_lbracket`, `scan_rbracket`,
`scan_backtick`), each named after its first-byte job, each
under CC 10. `scan_via_logos` itself is CC 6.

## What logos buys

1. The CC of `next_token` no longer dominates the file.
2. The operator table is declarative — adding a new structural
   operator means adding one variant and one `to_tok` arm.
3. Longest-match correctness for the structural subset is
   delegated to a well-tested DFA generator instead of being
   reproduced by hand in nested `if/else if` chains.

## What logos does not buy

Keyword promotion. `if`/`then`/`fn`/etc. still come out of
`scan_word`'s post-match lookup, because the word-boundary
semantics are entangled with the rest of word scanning. ~~The
keyword variants in `LogosTok` exist purely as guard rails — the
equivalence tests in `tests.rs` pin logos' word-boundary
behaviour on them, useful if we ever revisit that decision.~~ DD116 deletes
these rivals to the real keyword classifier.

## Open follow-ups

- If `scan_word` itself ever needs CC reduction, that's a
  separate exercise (the current refactor explicitly didn't
  touch it).
- If the per-call `LogosTok::lexer(slice)` construction shows up
  in profiles, switch to `Lexer::morph` or maintain a single
  lexer per scanner instance — the DFA itself is static.

## Addendum (2026-08-10) — `[[`/`]]` whitespace-optional, like `(( ))`

`[[` and `]]` now tokenize even when glued to an adjacent word or
its contents, so `if[[…]]`, `return[[…]]`, and the compact
interior `[[a==a]]` all parse — symmetric with `(( ))`, which was
already whitespace-optional (`(`/`)` are word-breaks).

Root cause of the old asymmetry: `[`/`]` are word characters (for
subscripts like `a[0]`), so `scan_word_chars` absorbed a bracket
that followed word bytes; `[[`/`]]` only reached the byte
dispatcher (→ `scan_lbracket`/`scan_rbracket`) when they led a
token after whitespace. `(`/`)` are in the word-break set, so
`((` always split.

Fix (`scan_word_chars`): break the word when the current bracket
is **doubled** — i.e. the next byte equals the current `[` or `]`.
A single `[`/`]` still absorbs as a subscript char. This is safe
because a doubled bracket only ever means a `[[ … ]]` delimiter in
sash: a variable/expression subscript index requires a `$` sigil
(`a[$i]`, `a[${b[0]}]`), so nested subscripts end in `}]`, never a
bare `]]`; the literal-bare `a[b[0]]` that would produce `]]` is
already rejected (SB4030). A codebase-wide search found zero glued
`[[`/`]]` in any sash source.

The same edit also converted the nested `if`s in `scan_word_chars`
(the `=`/`+`/bracket arms) to match guards, dropping its cognitive
complexity from ~14 to 3.

Not changed: the **dispatch** form still requires spaces around
`==`/`=~` (`[[ $x == a { … } ]]`); compact `[[ $x==a … ]]` is
SB4028. That is a dispatch-parser rule, independent of bracket
tokenization (the boolean test `[[a==a]]` accepts compact `==`).
Filed as `dispatch-compact-operator-spacing` if the inconsistency
is worth closing.

Specs: `spec/dispatch.sh` H1–H9 (`dp.ws1`–`dp.ws9`), covering the
compact/adjacent forms plus subscript and `(( ))` regressions.
