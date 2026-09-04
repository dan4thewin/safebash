title: DD#90 — brace-less disposition expansions (`$x:-foo`, `@a:~`)
status: accepted

Settled with DG (2026-09-04). The `{ }` in a disposition expansion is optional:
`$x:-foo` ≡ `${x:-foo}`, `@a:~` ≡ `@{a:~}`. Braces remain **available** (never
removed) and are **required** in the cases below.

## Motivation

DG: "I think the `{ }` in spellings like `${x:-foo}`, `@{a:~}`, etc are
unnecessary. They should only be needed when there is a trailing concatenation,
or the default/error message has unquoted whitespace, and perhaps in
interpolation." Subscripts are already brace-less (`$a[i]`, DD#41), so this
extends the same ergonomics to the disposition family.

## Scope — the disposition family ONLY

Brace-less applies to exactly the three disposition operators, for both a scalar
`$name` / positional `$N` and an array splat `@name`:

- **`:-`** default — `$x:-foo`, `$1:-foo`, `@a:-foo`
- **`:?`** error/abort — `$x:?msg`, `@a:?msg`
- **`:~`** elide — `$x:~`, `@a:~`

Substring `${x:off:len}` and regex `${x:s/re/sub/}` stay **braced** (DG
2026-09-04). Their `:digit` / `:s` forms collide badly with the common literal
colon-concatenation (`$x:3`), so the ergonomics aren't worth the ambiguity.

## Disambiguation — only `:-`/`:?`/`:~` open a disposition

After `$name` / `$N` / `@name`, a disposition opens **only** when the very next
bytes are `:-`, `:?`, or `:~`. Every other `:X` stays **literal colon
concatenation**, unchanged:

    $x:/bin   → "hi:/bin"     (path-like — preserved)
    $x:3      → "hi:3"        (colon-digit — preserved)
    $x:foo    → "hi:foo"      (preserved)
    $x:-foo   → ${x:-foo}     (disposition — reinterpreted)

Consequence (accepted, DG): `$x:-foo` / `$x:?…` / `$x:~` change meaning from the
old literal `$x` + `:-foo` to the disposition. This is a narrow reinterpretation
(nobody writes `$x:-`/`:?`/`:~` as a literal path); the literal reading is still
available as `${x}:-foo` or by quoting.

## Braces still REQUIRED when

1. **Trailing concatenation** after the operand — `${x:-foo}bar`. A brace-less
   operand runs to the word boundary, so `$x:-foobar` makes `foobar` the whole
   default; there is no way to append after it without braces.
2. **Unquoted whitespace** in the default/error word — `${x:-a b}`. Brace-less
   `$x:-a b` is default `a` then a separate word `b` (quotes are literal bytes,
   DD#65, so they do not group either — braces are the only way to a spaced
   operand).
3. **An embedded literal `}`** in the operand.
4. **A `$( )` / `$(( ))` / `` `…` `` substitution** in the operand — brace-less
   supports literal text and `$name` / `${…}` interpolation, but a command /
   arithmetic / backtick operand must be braced (`${x:-$(cmd)}`). A brace-less
   operand that reaches one falls back transparently to the pre-DD#90 tokenization.
5. **Interpolation inside `"…"`** — DG 2026-09-04, "err on the side of clarity":
   a bare `"$x:-foo"` is `$x` + the literal `:-foo`, NOT a disposition. Inside
   double quotes the disposition must be braced (`"${x:-foo}"`).

## Semantics = the braced form, exactly

Brace-less is pure sugar: the lexer desugars `$x:-foo` → `DollarBrace("x:-foo")`
and `@a:~` → `Word("@{a:~}")`, so validate / transform / codegen / all three
backends and every rule (taint SB5020, elide-in-concat SB5046, positional arity
SB5042/SB5048, `@{a:-}` = one empty arg per DD#69/#84, …) are byte-identical to
the braced spelling. Verified side-by-side (`$x:-foo` ≡ `${x:-foo}`).

## Implementation

`parse/lexer.rs` only (the desugar happens at tokenization):
- `scan_dollar` → `dollar_or_braceless_disposition`: after a `$name` / `$N`,
  `try_braceless_disposition` peeks for `:-`/`:?`/`:~` and
  `scan_braceless_operand` scans the operand to the word boundary (handling
  `$name` / `${…}` nesting; `None` → fall back to plain `Dollar`).
- `scan_word` → `try_scan_braceless_at_disposition` for the `@name:OP` array
  form → synthesized `Word("@{name:OP…}")`.
- The double-quote scanner is untouched, so rule #5 (interpolation) holds by
  construction.

Specs: `var_modifiers.sh` vm.bl1–bl12 (scalar + disambiguation + interpolation
exclusion + whitespace), `glob_splat.sh` gs.25–gs.30 (array). journal 242.

## See also

- DD#41 — `$a[i]` brace-less subscripts (the precedent).
- DD#65 — the surviving `${var:OP}` family and the `:- ≡ "…"` default body.
- DD#69 / DD#84 — the array-splat dispositions and the `:-`/`:~` distinction.
- Task `janet-positional-default-present` — a pre-existing janet bug surfaced by
  vm.bl4 (braced `${1:-DEF}` with an arg present also returns DEF in janet).
