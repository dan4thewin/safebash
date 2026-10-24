---
title: "Diagnostic: an `elif` clause records no position, so its errors point at the `if`"
status: open
priority: P3
kind: diagnostic
depends: []
gates:
  - "`ElifClause` carries the span of its `elif` keyword, set by the parser in both spellings"
  - "every parse error about an elif clause (empty condition, missing `{` body, missing `then`) reports the elif's position; specs pin p1, p3, q1 and q2 in both backends"
  - "every `If` that `flatten_elif` builds carries the elif's span; a unit test pins that none has `Span::default()`"
---

## What

`ElifClause` (`ir.rs`) records only `condition` and `body`. Two symptoms
follow, both present on base `1309385` and on the F13 worktree.

### 1. Parse errors in an elif report the `if`'s position (user-reachable)

| probe | program | reported | the error is at |
|---|---|---|---|
| p1 | `if /bin/true {…} elif {…}` (elif on line 3) | SB4027 at 1:1 | 3:3 |
| p3 | `echo x`, then `if … {…} elif /bin/false {…} elif {…}` (bad elif on line 6) | SB4027 at 2:1 | 6:3 |
| q1 | `if /bin/false {…} elif /bin/true` with no `{` body (elif on line 3) | SB4014 "expected '{' block body" at 1:1 | line 3 |
| q2 | `if /bin/false; then …; elif /bin/true` with no `then` (elif on line 3) | SB4014 "expected 'then' after elif" at 1:1 | line 3 |

`parse_if_brace` hands the `if` statement's span to `parse_brace_elif`,
which uses it for `brace_group_cond_error` and `parse_brace_block`.
`parse_if_keyword` passes the same span to `expect(…, "expected 'then'
after elif")`. (`parse_brace_else` receives it too; an else-side error was
not probed.) The `if`'s brace-form empty condition (p2) is reported
correctly, because there the `if` is the offending construct.

### 2. `flatten_elif` builds each nested `If` at `0:0` (latent)

`flatten_elif` (`transform/control_flow.rs`) turns each `elif` into an
`If` nested in the previous clause's else body, with
`span: Span::default()`. `lower_one` sets `VersionState::stmt_span` to the
statement being lowered, so a diagnostic raised at the `If` level, rather
than inside one of its child statements, would report `0:0`.

No user can reach this today, as far as the batch-531 port measured (F13).
Child statements keep their own spans (an elif arity error reports `5:8`,
probe e5; bodies report their own lines, e6–e8). validate and type_check
walk the original AST, which still has its elif clauses. The one If-level
transform diagnostic was `VersionState::define`'s SB4044 backstop on a
merge phi, which fired falsely on legal programs and printed `0:0` under
an elif (probe c53, reference branch). F13 fixed the false firing
(`define_merge`) and turned the backstop into an ICE.

## Fix

The parser records the `elif` keyword's span on `ElifClause` in both
spellings. Its elif errors use that span, and `flatten_elif` gives it to
the `If` it builds.

Using the first condition statement's span instead would need no IR
change, but it is not always there: the keyword spelling accepts an empty
elif condition (`q-empty-keyword-condition`), and it fixes only symptom 2.

Probes: `files/port-f13-probes.tar.gz`, directories `el/` (e5–e8) and
`el2/` (p1–p3, q1, q2).
