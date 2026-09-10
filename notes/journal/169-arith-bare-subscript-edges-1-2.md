# 169 — arith bare-subscript follow-ups: edges 1 & 2

`arith-bare-subscript-followups` edges 1 (`$a[i]` dollar form) and 2 (nested
`a[a[0]]`). DG chose the diagnostic route for edge 1 and directed me to stay on
edges 2 & 4.

## Edge 1 — `$(( $a[i] ))`: clear rejection, not a silent read

The Model B table lists `$a[i]` as an rvalue read "as today", so I first tried
to make it read the cell like `${a[i]}`. **That fix was unsafe and reverted.**

`$(( ))` content is collected by rebuilding a *string* from the logos `Tok`
stream (`collect_arith_content` → `tok_to_text`, with `needs_separator_space`
inserting spaces between tokens) and then re-tokenising it. The bare-ident
lexer fuses `a[i]` into one Word, but `$a` stops at the `[`, so `$a[i]` arrives
as `Dollar("a")` + `Word("[i]")`. Suppressing the space before the `[`-Word and
capturing `$name[…]` made the *literal* case `$a[1]` work — but any
`$`/variable index splits further: `$a[$i]` rebuilds as `"$a[ $i ]"` (spaces
land *inside* the brackets), so the index parsed wrong and it **silently read
a[0]**. Verified `$a[i]`, `$a[$i]`, `$a[i+1]` all returned index-0 values.
Reverted — a silent wrong answer is worse than the SB4035 reject.

DG's call: keep it rejecting, improve the diagnostic. `arith_bad_literal_error`
now special-cases a stray `[`/`]` (the shape a `$a[i]` reduces to after
reconstruction) with:

> `$a[i]` isn't a subscript read in arithmetic — use the bare cell form `a[i]`
> or the braced value form `${a[i]}`

instead of the confusing `invalid numeric literal '['`. Genuine bad numeric
literals (`08`, `0o8`) keep the numeric message. Specs dsx.1-2 (dsx.2 pins that
a variable index *rejects*, not silently reads).

A correct read-the-cell fix stays possible but is lexer-level (fuse `$name[…]`
in `scan_dollar`), which also changes the word-context `$a[i]` path — recorded
in the task for a future focused effort.

## Edge 2 — `$(( a[a[0]] ))`: nested bare subscript now parses

Root cause was in the **core word lexer**, not the arith re-parse.
`scan_word_chars` broke a word on any doubled `]]` (added so `[[ … ]]` is
whitespace-optional). A bare nested cell read `a[a[0]]` ends in `]]`, so it was
split into `Word("a[a[0")` + `DblBracketClose` — the close dropped, the
subscript corrupted → SB4035. The `[[ ]]`-safety note had assumed nested
subscripts always carry the `${…}` sigil and end in `}]`; bare model-B
subscripts violate that.

Fix: track subscript-bracket depth in `scan_word_chars`. A doubled `[[`/`]]`
delimits a test/dispatch only at depth 0; inside a subscript (depth > 0) `]]` is
two subscript closes. Now `a[a[0]]` (→9), triple-nesting `a[a[a[0]]]` (→0), and
the lvalue `(( a[a[0]] = 5 ))` all work; a string-array inner cell still hits
the DD#77 SB4030 disposition gate. `[[ ]]` dispatch is unaffected — dispatch.sh
green, 0 regressions. Specs nsx.1-3 (native,c; janet arith-value subscript gap).

### Adjacent finding filed (not fixed here)

The *braced* nested form `${a[a[0]]}` silently returns the inner cell (`a[0]`)
rather than the outer lookup. It runs through `scan_brace_expansion` +
`subscript_index_expr`, independent of `scan_word_chars`, so it predates this
change. Filed `bug-braced-nested-subscript-index.md`.

## Verification

Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs HEAD
(`44043a1`): 0 passing→failing, 0 gone, 12 new passing (dsx.1-2 ×3 modes,
nsx.1-3 ×native,c). `[[ ]]` forms spot-checked (compact `[[x==x]]`, dispatch
arms, `return[[…]]`).

## Remaining

Edge 4 (substring `${a[i]:o:l}` / sed `${a[i]:s/…}` on a cell → scalar parity,
currently SB4039) — next.
