# 141 — `[[ ]]` whitespace-optional, like `(( ))`

DG spotted an asymmetry from a demo: `fn foo{return((1==1))}…` parsed, but
`fn foo{return[[a=a]]}…` did not. Diagnosis: `(( ))` was whitespace-optional
while `[[ ]]` required surrounding *and* interior spaces.

## Root cause

`scan_word_chars` (lexer.rs) treats `[`/`]` as **word characters** (for
subscripts `a[0]`), whereas `(`/`)` are in the word-break set. So `[[`/`]]`
only reached the byte dispatcher (→ `scan_lbracket`/`scan_rbracket`) when a
bracket *led* a token after whitespace; glued to a word it was absorbed:

- `return[[…` → `[` pulled into `return[[…` → the `[[` never tokenizes.
- `[[a==a]]` → `[[a` and `a]]` glued → no `]]` token.

`(( ))` had no such issue because `(` always breaks a word.

## Why the fix is safe (DG pushed on this)

The obvious worry — a doubled `]]` colliding with a nested subscript — turns
out moot. In sash a variable/expression subscript index **requires a `$`
sigil** (`a[$i]`; bare `a[i]` is SB4030 string-key). A nested subscript is
therefore `a[${b[0]}]`, which ends in `}]`, never a bare `]]`. The only form
that yields a bare `]]` is the literal `a[b[0]]`, already rejected. A
codebase-wide search (spec/, sash/tests, sash-convert, sash-expand) found
**zero** glued `[[`/`]]` in any sash source (only TOML `[[package]]` headers
and vendored oils/shellcheck corpora, neither run by the gate).

## The change

`scan_word_chars`: break the word when the current bracket is **doubled**
(next byte equals the current `[`/`]`). A single bracket still absorbs as a
subscript char. One arm, one comparison — `[[ … ]]` is now whitespace-
optional in every position (`if[[…]]`, `while[[…]]`, `![[…]]`, compact
`[[a==a]]`, and DD#74 `return[[…]]`).

Same edit converted the arm's nested `if`s (the `=`/`+`/bracket cases) to
**match guards**, dropping `scan_word_chars` cognitive complexity 14→3 (it was
a grandfathered gate violator; touching it forced the reduction, done by
guard-conversion rather than a bypass).

## Scope note

The **dispatch** form still requires spaces around `==`/`=~`
(`[[ $x==a { … } ]]` → SB4028), unlike the boolean test which accepts
`[[a==a]]`. That's a dispatch-parser rule, independent of bracket
tokenization; filed as `dispatch-compact-operator-spacing` (DG scoped this
change to `[[`/`]]` only).

## Verification

`spec/dispatch.sh` H1–H9 (`dp.ws1`–`dp.ws9`): compact/adjacent boolean +
dispatch forms, `return[[…]]`, and subscript / `(( ))` regressions. Full
`native c janet` sweep vs HEAD: **0 regressions, 27 new passing** (9×3).
Rust suites: identical failure set to HEAD (28 pre-existing `brush_compat`
rot; 0 new). Recorded in DD#59 addendum.
