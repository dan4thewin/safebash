---
title: "brace-less disposition expansions: $x:-foo, @a:~ (DD#90)"
date: 2026-09-04
---

# Brace-less dispositions (DD#90)

DG: "I think the `{ }` in spellings like `${x:-foo}`, `@{a:~}`, etc are
unnecessary. They should only be needed when there is a trailing concatenation,
or the default/error message has unquoted whitespace, and perhaps in
interpolation."

## Design conversation first

This is a language-surface change with a real conflict, so I settled the design
with DG before implementing (decision authority). Findings I brought:

- **Precedent:** `$a[i]` subscripts are already brace-less (DD#41).
- **The conflict:** `:` after `$x` is currently literal concatenation —
  `$x:/bin` → `hi:/bin`, `$x:3` → `hi:3`, `$x:-foo` → `hi:-foo`. Making `:`
  dispositions brace-less reinterprets some of these. Severity depends on scope:
  the disposition family (`:-`/`:?`/`:~`) has negligible literal-concat conflict;
  substring `:off:len` (`$x:3`) and regex `:s/` collide badly.
- The `}` also currently *terminates* the default body (DD#65), so brace-less
  must instead terminate at a word boundary — which is exactly why DG's
  exceptions (trailing concat, embedded whitespace) need braces.

DG's decisions (ask_user): **disposition family only** (`:-`/`:?`/`:~`), scalar
+ array; the disambiguation rule (only those three open a disposition, every
other `:X` stays literal); the exceptions — **and interpolation inside `"…"`
keeps braces mandatory** ("err on the side of clarity"). Substring/regex stay
braced.

## Implementation — a lexer desugar

Pure sugar at tokenization, so nothing downstream changes:

- `$x:-foo` → `DollarBrace("x:-foo")`, `$1:-foo` → `DollarBrace("1:-foo")`,
  `$x:~` → `DollarBrace("x:~")` (`scan_dollar` → `dollar_or_braceless_disposition`
  → `try_braceless_disposition` + `scan_braceless_operand`).
- `@a:-foo` → `Word("@{a:-foo}")`, `@a:~` → `Word("@{a:~}")`
  (`scan_word` → `try_scan_braceless_at_disposition`).

The operand scans to the word boundary, consuming `$name` / `${…}` interpolation
(quotes are literal bytes, DD#65); a `$( )` / `$(( ))` / `` `…` `` operand returns
`None` and the lexer falls back to the pre-DD#90 tokenization (→ use braces). The
double-quote scanner is untouched, so the interpolation exclusion (#5) holds by
construction — `"$x:-foo"` stays `$x` + literal `:-foo`.

Because it desugars to the braced token, every downstream rule is byte-identical:
taint (SB5020), elide-in-concat (SB5046), positional arity (SB5042/SB5048), the
`@{a:-}` = one-empty-arg / `@{a:~}` = elide distinction (DD#69/#84). Verified
side-by-side: `$x:-foo` ≡ `${x:-foo}`, `@a:-none` ≡ `@{a:-none}`.

## Disambiguation preserved

`$x:/bin` → `hi:/bin`, `$x:3` → `hi:3`, `$x:foo` → `hi:foo` — all unchanged
(only `:-`/`:?`/`:~` trigger). `$x:-foo` changes from literal `hi:-foo` to the
default expansion (the accepted narrow reinterpretation).

## One pre-existing bug surfaced

`vm.bl4` (`fn f { echo $1:-DEF }; f VAL`) failed in **janet** (returned DEF, not
VAL). The braced `${1:-DEF}` emits **identical janet** and has the same bug — a
pre-existing janet positional-default defect, not a sugar issue. Gated `native,c`;
filed `janet-positional-default-present`. native/c correct.

## Verification

- `var_modifiers.sh` vm.bl1–bl12: scalar `:-`/`:?`/`:~`, positional, operand
  interpolation, brace-less≡braced, disambiguation (`$x:/bin`, `$x:3`),
  interpolation-excluded (`"$x:-foo"` literal), whitespace-ends-operand. plan
  57→69.
- `glob_splat.sh` gs.25–30: array `@a:-`/`@a:?`/`@a:~`, empty/non-empty,
  brace-less≡braced. plan 24→30.
- Full 3-mode sweep vs HEAD (`2e48463`): **0 regressions**, 54 new (53 passing,
  1 janet skip). Parser unit + logos-equivalence tests pass. fmt/clippy clean.

DD#90 written. No new SB code (reuses the braced diagnostics).
