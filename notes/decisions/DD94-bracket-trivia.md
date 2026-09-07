---
title: DD#94 — newlines, `;` and comments as trivia inside `(( ))` and `[[ ]]`
status: accepted
---

Settled with DG (2026-09-07), from the deferred task
`notes/tasks/arith-block-separators-and-comments.md` (decisions 1b, 2b, 3b)
plus DG's addition of line breaks and comments inside a dispatch block. DG
chose the full package after reviewing worked examples.

## Motivation

sash already treats newlines and comments as trivia inside most bracketed
constructs — array literals `( … )`, function and arm bodies `{ … }`, and
after a pending `|` / `&&` / binary operator. Three holes remained, and they
forced unreadable one-liners:

| construct | newline (before) | comment (before) |
|---|---|---|
| array literal `( … )` | ok | ok |
| body `{ … }` | ok | ok |
| `[[ … ]]` **test** | SB4014 | ok |
| `[[ … { } ]]` **dispatch**, between arms | SB4014 | SB4014 |
| `(( … ))` after a complete sub-expr | SB4020 | SB4035 |
| `(( … ))` `;` | SB4035 | — |

Two of those rows are plainly inconsistent with each other: a comment was fine
inside a `[[ ]]` *test* but not between *dispatch* arms — both are `[[ … ]]` —
and an array literal could span lines while an arm list could not.

bash rejects all of the "before" cases too, so this is a **deliberate
divergence**, taken because sash's ethos is a safe subset that *reads clearly*,
and because DD#85 already made comments uniform trivia everywhere else.

## The rule

Inside `(( … ))` and `[[ … ]]`, **newlines and comments are trivia**. Neither
construct is newline-terminated — each is closed by its own delimiter — so a
newline inside carries no information.

### Arithmetic: newline and `;` sequence like `,`

A standalone `(( ))` is a sequence block; the value is the last expression.

    (( total = price * qty
       tax   = total * 7 / 100
       grand = total + tax ))

`,`, `;` and newline are interchangeable there. **Exception:** the C-style
for-header reserves `;` to delimit its three slots — `parse_c_style_for` splits
the raw header on `;` *before* the arith parser runs — so within a slot only
`,` and newline sequence.

**The continuation rule.** A newline separates only after a **complete**
sub-expression. With an operand still owed (a binary operator or `=` pending)
it is a line continuation:

    (( x = 1 +
         2 ))      # → 3, one expression (as before)
    (( a = 1
       b = 2 ))    # → two, newline separates

This is the standard significant-newline treatment (Go / JS ASI). It falls out
of one rule in the parser: the operand position skips separators, so a pending
operator swallows them, while a completed expression ends at one.

A separator is **required** between expressions, so `(( 1 2 ))` remains the
error it always was rather than silently becoming a sequence.

### Arithmetic: comments

A whitespace-preceded `#…EOL` is a comment inside `(( ))` and `$(( ))`.

A `#` **immediately after an alnum** is still the DD#72 base-literal marker and
still a loud `SB4035` — `16#ff` must never silently truncate to `16`. That
adjacency test is the whole safety story for this change.

**Consequence:** a comment runs to end of line, so a `))` on a commented line
is part of the comment. `(( a = 1 # x ))` is therefore *unterminated*, and
that is now a hard error rather than a silent truncation (see below).

### `[[ ]]`: line breaks and comments

Arms may be laid out one per line, with comments between them:

    [[ $cmd == start   { start_service; }
              stop     { stop_service; }
              # anything else is a usage error
              || { usage; } ]]

and a test may span lines around `&&` / `||` / an operand:

    if [[ -n $host &&
          -n $port ]]; then

**A newline ends one arm's pattern list.** `foo ⏎ bar { … }` is two arms, not
one arm with two patterns — patterns of a single arm stay on one line. That is
what keeps a multi-line arm list unambiguous, and it is why the skipping is
done at the parser's structural points rather than in the scanner: an arm
**body** is an ordinary statement list, where newlines still separate
statements.

## What does NOT change

- The C-style for-header (`;` still delimits its three slots).
- `16#ff` and every other `base#digits` form — still SB4035 (DD#72).
- `(( 1 2 ))` — still an error; a separator is required.
- Arm bodies and every other `{ … }` block — newlines still separate
  statements.
- `$(( ))` rvalues still reject assignments (SB4020), so the sequence-block
  form is about standalone `(( ))` statements.

## Also fixed here

An **unterminated** `(( ))` / `$(( ))` was silently accepted: the collector
returned whatever it had at EOF. With comments now able to eat a closing `))`
that became reachable in ordinary code, so it is a hard error naming the likely
cause. Previously `echo $((5 # 3))` printed `5`; it now fails. This is the
DD#72 principle (a malformed literal is a loud reject, never a silent
truncation) applied to the delimiter.

## Implementation notes

- `tok_to_text` mapped `Tok::Newline` → `" "`, erasing newlines before the
  arith re-lexer could see them; it now emits `"\n"`.
- `tokenize_arith` emits `ATok::Sep` for `\n` and `;`; `parse_prefix` skips
  separators (the continuation rule), and `fold_sequence` generalises the old
  comma-only fold.
- `arith_hash_word` (`parse/lexer.rs`) now emits the DD#72 Word only when the
  `#` is alnum-adjacent; otherwise it falls through to the normal comment path.
- `[[ ]]`: `skip_test_newlines` at the structural points — `parse_dispatch`'s
  loop head, before each arm's brace block, `parse_test_or`/`_and`/`_expr`, and
  `parse_binary_rhs` (where an operand is owed after an operator).

## Specs

`spec/arithmetic.sh` 94.1–94.10 (separators, comments, and the guards:
continuation, `16#ff`, for-header) and `spec/dispatch.sh` 94.20–94.28 (arm
layout, comments, spanned tests, and the guards: pattern-list-ends-at-newline,
arm bodies still statement-separated). 94.10 is gated `native,c` for a
pre-existing janet gap (`janet-for-init-comma-sequence`).
