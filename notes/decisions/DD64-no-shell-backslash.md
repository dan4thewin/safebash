title: DD#64 — No shell-level backslash escaping; quoting + concatenation instead
status: accepted

The shell interprets backslash in **exactly two** places — `$'…'`
(C-escapes) and `\` at end-of-line (line continuation). Everywhere else —
unquoted, `'…'`, `"…"` — `\` is a verbatim literal byte. Escaping shell
metacharacters is done by **quoting and string concatenation**, not
backslashes. From the 2026-06-17/18 discussion.

## Ratified (DG, 2026-06-21)

Accepted as written, **including** the breaking change that inside `"…"`
`\\` no longer collapses to one backslash and there is no `\"`/`\$` — a
deliberate divergence from bash, "judged worth the consistency." The core
(`scan_dq_escape`) is already implemented; the in-`${var:OP}` backslash
handlers are removed by DD#63 (Path B) and DD#65 (operator drops). No
open design items.

## Implementation status (2026-06-19)

**Done** — the core change is `lexer.rs::scan_dq_escape`: inside `"…"`,
`\` is now a verbatim byte (only `\<newline>` continuation survives), so
`"a\\b"`→`a\\b`, `"a\$x"`→`a\`+interp, `"a\"…`→`a\`+close. Spec tests
`spec/wordsplit.sh` 64.1–64.3. Verified zero net regression (native specs
417/2 unchanged; rust clean baseline 186, only flaky `oracle_pipeline`).

**Already compliant (no change needed)** — bare words (`scan_word_chars`
pushes `\` verbatim and breaks at spaces, so `a\b`→`a\b`, `a\ b`→two
words) and regex-within-a-word (`a\.b` tokenises as one `Word`, the `\`
survives). The DD over-counted these as removals.

**Still deferred** to DD#63/DD#65 — the brace-scan `\}` (lexer.rs:660,
plus the in-`"…"` `${}` skip at ~824 and `skip_balanced_dquote`) and the
`/`-operator `\/` (`find_unescaped_slash`). These are `${var:OP}`
brace/glob-matching machinery removed when Path B (DD#63) and the
`/`-glob-replace drop (DD#65) land.

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

## Resolved by the var-modifier vet (DD#65) + Path B (DD#63)

How `\` behaves *inside* `${var:OP}` operands is now settled, and **both**
in-operand backslash handlers are removed:

- The `/`-operator `\/` (`find_unescaped_slash`, expr.rs:741) goes with the
  `/`-glob-replace op itself — DD#65 **drops** `${v/a/b}` (redundant with
  `sub`/`gsub`), which deletes `find_unescaped_slash`.
- The brace-scan `\}` (lexer.rs:660) is **removed outright** — no kept
  operator needs it. `:s/…}` protects a bare `{`/`}` via **Path B**
  (DD#63: the lexer treats `/` as a quote-delimiter after `:s`); `:-`
  includes a literal `}` by **quoting** (`${foo:-'a}b'}` → `a}b`, since the
  lexer already skips `'…'`); `:off:len` takes integer operands. A literal
  `}` that still needs a byte escape is `\x7d` (DD#63 `\xnn`), but it is no
  longer load-bearing for either kept operator.

So the `${var:OP}` family is fully clean under "no shell backslash": the
only backslash interpreters left anywhere are `$'…'`, `\`-at-EOL, and the
regex engine.

## See also
- DD#63 — regex builtins; `\` in regex args follows this DD.
- DD#62 — `[[ ]]` simplification.
