title: DD#64 — No shell-level backslash escaping; quoting + concatenation instead
status: candidate

The shell interprets backslash in **exactly two** places — `$'…'`
(C-escapes) and `\` at end-of-line (line continuation). Everywhere else —
unquoted, `'…'`, `"…"` — `\` is a verbatim literal byte. Escaping shell
metacharacters is done by **quoting and string concatenation**, not
backslashes. From the 2026-06-17/18 discussion.

> Scope: this covers backslash in *string* and *regex-argument* contexts.
> Backslash *inside* `${var:OP}` expansions — the brace-scan `\}` and the
> `/`-operator `\/` (`find_unescaped_slash`) — is **deferred to the
> var-modifier vet** (`vet-var-modifiers`), since several of those
> operators may be dropped.

## Why

Backslash is currently handled in ~7 inconsistent, context-dependent ways
(line-continuation, brace-scan skip, `scan_dq_escape`, single-quote
verbatim, ANSI-C, word-char, `find_unescaped_slash`), producing warts:
the `"\\"`→`\` collapse and the awful `"\\\\"` for one backslash; `a\ b`
not equal to `'a b'`; `\/` broken in `:s/`; a standalone `\` silently
dropped on `=~`. Collapsing to "verbatim except `$'…'` and EOL" removes
the warts and the brittle code.

## The rule

- **`$'…'`** — C-escapes (`\n`, `\t`, `\xNN`, …). Kept; the one place the
  *shell* reads backslash escapes.
- **`\` at EOL** — line continuation. Kept (structural).
- **unquoted / `'…'` / `"…"`** — `\` is a verbatim literal byte. No `\"`,
  no `\$`, no `\\`-collapse, no `\ `, no word-char escaping. `'…'` and
  `"…"` differ *only* by `$`-interpolation.

Two backslash *interpreters* remain, both downstream and contextual:
`$'…'` (shell C-escapes) and the regex engine (`\.`, `\\`). The shell
proper interprets none.

## Escaping by quoting + concatenation

Adjacent single-quoted literals and interpolations concatenate (verified:
`'"'$foo'"'` → `"bar"`, `'$'X` → `$X`, `'<'$x'>'` → `<hi>`), which
replaces every backslash escape:

| want | write |
|------|-------|
| literal `"` | `'"'` |
| literal `$` | `'$'` |
| literal `\` | `'\'` (or verbatim in any quote) |
| a space | `'a b'` / `' '` |
| literal byte in a regex (space, `/`, `}`, `\`) | `\xNN` (`\x7d` = `}`, `\x5c` = `\`) |
| newline etc. | `$'\n'` |

## Consequences

- `"a\\b"` → `a\\b` (two literal backslashes; **no collapse**) — a
  breaking change from current double-quote semantics. sash is not
  bash-compatible; the change is judged worth the consistency.
- A regex in single quotes is *standard regex escaping*, untouched by the
  shell: `'a\.b'`, `'a\\b'`, `'\(x\)'`. The recommended form.
- In double quotes (for interpolation) `\.`/`\(` still reach the engine
  verbatim; a literal backslash is `\x5c`, never `"\\\\"`.
- Removes `scan_dq_escape`, the `parse_regex_rhs` standalone-`\` drop, and
  the word-char escape. (`$'…'` and EOL handlers stay.)

## Deferred to the var-modifier vet

How `\` behaves *inside* `${var:OP}` operands — the brace-scan `\}`
(lexer.rs:660) and the `/`-operator `\/` (`find_unescaped_slash`,
expr.rs:741) — is resolved during the vet of the `${var:OP}` family
(`vet-var-modifiers`); some operators (e.g. the `/` glob-replace) are
drop candidates. The brace `}` residual is covered there (`\x7d`, a
variable, or the `$(sub …)` form).

## See also
- DD#63 — regex builtins; `\` in regex args follows this DD.
- DD#62 — `[[ ]]` simplification.
