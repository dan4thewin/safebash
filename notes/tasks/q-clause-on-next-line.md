---
title: "Question: may an `elif` or `else` start the line after a brace body's `}`?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules; the ruling is recorded in DD#68's Forms"
  - "every program in the tables compiles and runs as the ruling says, or is rejected with a diagnostic that states the rule, in both backends"
  - "the SB4072 `else` and `elif` messages (`stray_keyword_message`, sash/src/parse/parser.rs) state the ruled form, and skw.20 pins it"
---

## What

Found 2026-10-27, while porting F1+F4 (journal 537). A brace-form `if` ends
at a body's `}` unless the next token is `elif` or `else`
(`parse_if_brace`, sash/src/parse/parser.rs:913-936). A newline is a token,
so a clause keyword on the next line is not part of the `if`. It stands at a
statement head instead, where F1+F4 made it SB4072:

| Id | Program | On 8e28c61 | With F1+F4 |
|---|---|---|---|
| m4 | `if /bin/true {` / `  /bin/echo a` / `}` / `else {` / `  /bin/echo b` / `}` (skw.20) | SB5011 "command 'else' not found" at 4:1 | SB4072 at 4:1, the `else`, and a cascade SB4072 at 6:1 |
| r7 | m4 with `elif /bin/false {` on line 4 | SB5011 at 4:1 | SB4072 at 4:1, and the cascade at 6:1 |

So the form was never accepted; F1+F4 changed only the diagnostic. Its
message states the rule: "after a brace-form body it must directly follow
that body's closing `}`, on the same line (`} else {`)". No DD states it.
DD#68's Forms (line 15) write the clause on the `}`'s line without saying it
must be there. The cascade at 6:1 is the second shape of
`bug-parse-error-in-block-cascades-sb4072`.

This is the only newline either form refuses. Every other newline in an `if`
or a loop is accepted, on 8e28c61 and with F1+F4, in both backends:

| Id | Program | Prints |
|---|---|---|
| b1 | `if /bin/false {` / `  echo a` / `} else` / `{` / `  echo b` / `}` | `b` |
| b3 | `if /bin/true` / `{` / `  echo a` / `}` | `a` |
| b4 | `while /bin/true` / `{` / `  echo a` / `  break` / `}` | `a` |
| k1 | `if /bin/true; then` / `  echo a` / `else` / `  echo b` / `fi` | `a` |
| k2 | `if /bin/true` / `then` / `  echo a` / `fi` | `a` |

So a program written in Allman style, with each `{` on a line of its own,
compiles until its first `else`, which has to move back onto the `}`'s line.
sash-convert is unaffected: it writes the keyword form, where `else` on a
line of its own is fine (k1).

Precedents: Go refuses a newline before `else` and before a block's `{`,
because a newline there ends the statement. C, Java, JavaScript and Rust
accept both. Bash has no brace form; its keyword form accepts a newline
before `then`, `do`, `else`, `elif` and `fi`.

## Options

- (a) Keep the rule and write it into DD#68's Forms. The one refused newline
  stays an exception, which the message already explains. Once the cascade
  task lands, m4 and r7 are a lone SB4072.
- (b) Accept a clause after newlines. `parse_if_brace` looks past newline
  tokens for `elif` or `else`, and consumes them only when one follows.
  Nothing becomes ambiguous, because neither keyword can start a statement
  (F1+F4 made both SB4072 everywhere). m4 and r7 compile; skw.20 and the two
  messages are rewritten. A second `else` (r8 in the cascade task) is still
  SB4072.
- (c) Refuse every such newline, as Go does, so there is one placement. b1,
  b3 and b4 stop compiling, and nothing measures how many user programs that
  breaks.

Recommendation: (b). The brace form already accepts Allman placement for
every other brace, so this is the one place a user meets the rule, and
accepting it costs no ambiguity. (a) is the no-change answer. DG has not
ruled.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks (2026-10-24).

Probes: `files/port-f1f4-probes.tar.gz`, directories `kw5/` (m4), `kw2/`
(r7) and `cl/` (b1, b3, b4, k1, k2).
