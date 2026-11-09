---
title: "Allow brace-form else/elif clauses on following lines"
status: open
priority: P3
kind: work
depends: []
gates:
  - "native and C accept next-line else/elif, including repeated elif clauses and blank/comment lines, with the same branch behavior as the same-line forms"
  - "preserve existing nested ownership, else-if desugaring, header/body placements and keyword forms; a non-clause successor keeps its newline boundary"
  - "semicolon- or statement-separated clauses, genuinely stray clauses and further clauses on an if that already has an else remain invalid; do not broaden next-line attachment for operators, redirects or &"
  - "SB4072 else/elif advice describes the accepted placement instead of requiring the same line, and diagnostics retain the actual clause's source position"
  - "migrate skw.20's changed contract at fresh ids; acceptance of m4/r7 is not evidence that general block-error recovery was repaired"
  - "prove new behavior with real CLI before/reversion controls and a predicted gate; preserve existing legal forms without unrelated diagnostic masking"
---

## Accepted policy (DG, 2026-11-09)

DG chose the clause-only exception after discussing a uniform
complete-statement newline rule and the possible leading-operator,
redirect and background-mark extensions:

> if next-line else / elif can be had cheaply, let's allow it

DD68's Forms are authoritative. An `elif` or `else` may follow the
preceding brace-form body across one or more newlines, including blank
and comment-only lines. No backslash is required. An explicit `;`,
another statement or an intervening non-clause token is not trivia.
Existing clause ownership and nesting remain; a completed `else` does
not acquire another clause.

This grants no general next-line attachment to pipes, `&&`/`||`,
redirects or `&`, and adds no clause-bearing construct. Existing
same-line clauses, accepted header/body layouts, `else if` desugaring,
keyword forms and explicit continuation keep their contracts. Capture/
attest adjacency does not need its binding boundaries erased.

The policy question is closed; **implementation remains open** here.
DD71, GUIDE and journal 601 record the boundary and ownership.

## Bounded implementation

Source inspection at `6187dc14` confirms a local parser route:

- `parse_if_brace` (`sash/src/parse/parser.rs:900-923`) checks the
  immediate token after a body for `Tok::Elif`/`Tok::Else`.
- `Scanner::peek_n` (`sash/src/parse/lexer.rs:443-449`) provides
  buffered lookahead; `next_token_skip_comments` (530-537) already
  removes comment tokens.
- Look past only newline tokens, and consume them only if a clause
  follows. Otherwise leave the boundary for the enclosing parser.
  Do not use `skip_terminators` for this: it consumes semicolons too.
- Preserve `clause_keyword_span` (parser.rs:858-861) at the actual
  keyword. The existing AST and downstream branch semantics suffice.

This is an implementation-cost assessment, not a tested patch or a
performance measurement. No lexer redesign, new AST form or backend
semantics are selected. Converter output need not adopt the new layout;
preserve its existing valid forms rather than changing formatting policy.

`stray_keyword_message` must stop claiming that a clause must be on the
same line. Keep genuine stray/duplicate-clause diagnostics, rather than
hiding SB4072 globally. The separate
`bug-parse-error-in-block-cascades-sb4072` owns recovery after actual
errors: m4/r7 become valid programs, while r8's second `else` remains
invalid. Coordinate skw.20's migration with that task.

## Historical evidence (not rerun for the policy recording)

Found 2026-10-27, while porting F1+F4 (journal 537). The immediate-token
check ended the `if` at a newline; the following clause keyword then
stood at statement head, where F1+F4 made it SB4072:

| Id | Program | On 8e28c61 | With F1+F4 |
|---|---|---|---|
| m4 | `if /bin/true {` / `  /bin/echo a` / `}` / `else {` / `  /bin/echo b` / `}` (skw.20) | SB5011 "command 'else' not found" at 4:1 | SB4072 at 4:1, the `else`, and a cascade SB4072 at 6:1 |
| r7 | m4 with `elif /bin/false {` on line 4 | SB5011 at 4:1 | SB4072 at 4:1, and the cascade at 6:1 |

Both measured versions rejected those forms; F1+F4 changed only the
diagnostic. Its same-line advice described a parser limitation, not an
explicit DD placement rule. Git history traces the same immediate-token
check to hand-parser brace support in `217b25d1` (2026-07-04); that
commit message supplies no same-line rationale.

The following other placements were accepted on 8e28c61 and with
F1+F4, in both backends:

| Id | Program | Prints |
|---|---|---|
| b1 | `if /bin/false {` / `  echo a` / `} else` / `{` / `  echo b` / `}` | `b` |
| b3 | `if /bin/true` / `{` / `  echo a` / `}` | `a` |
| b4 | `while /bin/true` / `{` / `  echo a` / `  break` / `}` | `a` |
| k1 | `if /bin/true; then` / `  echo a` / `else` / `  echo b` / `fi` | `a` |
| k2 | `if /bin/true` / `then` / `  echo a` / `fi` | `a` |

m4/r7 now require acceptance; b1/b3/b4/k1/k2 remain preservation cases.
Use fresh spec ids for changed contracts, with same-line, nested,
semicolon-separated and non-clause-successor controls. The historical
table is not a new gate prediction or a complete newline-placement census.

The original filing was a question under DG's port rule (2026-10-24).
The rejected alternatives were retaining same-line clauses or retiring
other accepted layouts too. The new allowance does not infer a general
leading-operator rule from clause formatting.

Probes: `files/port-f1f4-probes.tar.gz`, directories `kw5/` (m4), `kw2/`
(r7) and `cl/` (b1, b3, b4, k1, k2).
