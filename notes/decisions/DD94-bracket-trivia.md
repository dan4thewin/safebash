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

`,`, `;` and newline all sequence there, but they are **not**
interchangeable — see the amendment below. `,` *separates* (an expression is
owed on each side); `;` and newline *terminate* (a run of them is trivia).
**Exception:** the C-style for-header reserves `;` to delimit its three slots
— `parse_c_style_for` splits the raw header on `;` *before* the arith parser
runs — so within a slot only `,` and newline sequence.

**The continuation rule.** A newline separates only after a **complete**
sub-expression. With an operand still owed (a binary operator or `=` pending)
it is a line continuation:

    (( x = 1 +
         2 ))      # → 3, one expression (as before)
    (( a = 1
       b = 2 ))    # → two, newline separates

This is the standard significant-newline treatment (Go / JS ASI). It falls out
of one rule in the parser: the operand position skips *terminators*, so a
pending operator swallows them, while a completed expression ends at one. A
`,` is not swallowed — `1 + , 2` is a missing operand.

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

## Amendment (DG, 2026-09-24): `,` separates, `;` and newline terminate

"`,`, `;` and newline are interchangeable" overreached. DG: *"I see `,` as an
expression separator and `;` and newline as a statement terminator. However,
there doesn't seem to be much difference between a statement and an expression,
if any. The bounding `((` and `))` means that all intervening
statements/expressions must be arithmetic."*

That last point is right, and the distinction survives it anyway — because the
difference is not *what they join* but **how they treat emptiness**:

- A **terminator** says "that one is done". Repeating it terminates nothing
  twice, so a run of them is harmless — blank lines, `;;`, an empty statement.
- A **separator** asserts an expression on *each* side. An empty side is a
  contradiction. `f(a,,b)` and `1 + , 2` are legal in no language.

A second asymmetry follows: a pending operator may swallow a *terminator*
(Go/JS ASI — "clearly not over"), but swallowing a *separator* is incoherent,
since `1 +` is missing an operand by definition.

The original rule generalised the comma-fold to `;` and newline — which was the
point — and then, for symmetry, let `,` inherit the *terminator* privileges
(run-collapsing, and being swallowed in operand position) it never had. The
cost was real: malformed arithmetic became a **wrong answer** rather than an
error, the exact failure mode SB4020 was hardened against.

| input | before | now | bash |
|---|---|---|---|
| `$(( 1 + , 2 ))` | **3** | SB4020 | error |
| `$(( , 1 ))` | **1** | SB4020 | error |
| `$(( 1 ,, 2 ))` | **2** | SB4020 | error |
| `$(( ,, ))` | **0** | SB4020 | error |
| `$(( 1, ))` | **1** | SB4020 | error |

Every readability feature the DD was taken for is unaffected: newline
continuation, blank lines between expressions, blank lines mid-continuation,
mixed `,`/`;`/newline blocks, and the trailing-terminator form `(( a=1 ⏎ ))`.
`(( a = 1 ⏎ , b = 2 ))` is still accepted — the newline is trivia and both
sides of the comma are real expressions.

The all-separators escape in `parse_arith_str` (which returns 0, and exists so
a C-style `for (( ;; ))` header parses) now admits **terminators only**; that
is what had made `$(( ,, ))` an empty expression.

Specs `94.31`–`94.40` in `spec/arithmetic.sh`. Found while chasing
`atok_to_text`'s cold arms for coverage — journal 398.

## Enforcement correction (2026-10-16)

Quoted arithmetic had a private boundary scanner and handed raw text directly
to the arithmetic re-lexer. It neither recognized DD#94 comments nor enforced
balanced `${…}` bodies: `int x=5; echo "$(( ${x ))"` silently printed `5`.
Quoted and unquoted arithmetic now share token collection, including comment
handling and lexical errors.

Closing parentheses are individual tokens. A group/call close can be adjacent
to the outer `))` without consuming part of it: `$(((1+2)))` evaluates to `3`,
quoted or not. The outer delimiter still requires adjacent closes; a spare
single `)` remains expression text and gets the ordinary arithmetic diagnostic.
Specs `aclose.1`–`aclose.20` and `arithcold.1`–`arithcold.8`; journal 515.

The collector also preserves physical token adjacency rather than guessing
which shell-token pairs form an arithmetic operator. Compact `p!=0` must stay
compact; whitespace in `p! =0` or `p- =2` must not manufacture `!=` or `-=`.
The old private operator-suffix list did both: it split valid inequality and
joined a separated subtraction/assignment into a silently accepted compound
assignment. Specs `arithadj.1`–`arithadj.6`; journal 516.
