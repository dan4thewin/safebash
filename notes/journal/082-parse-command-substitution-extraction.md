# 082 — parse_command_substitution extraction

## Goal

Reduce `parse/expr.rs::parse_command_substitution` (CC 22) by
honouring the audit recipe: extract `try_special_word(scanner, span)
-> Option<Expr>` for the inline `status` / `getcpid` cases. Also
lift the `$(<file)` slurp form out since it's a self-contained early
return.

## Approach

Two-stage probe inside the main loop:

```
if stmts.is_empty()
   && let Tok::Word(w) = tok
   && let Some(outcome) = try_special_word(scanner, span, w)? {
     match outcome {
        SpecialWord::Returned(e) => return Ok(e),
        SpecialWord::Continued(s) => { stmts.push(s); continue; }
     }
   }
```

The outcome enum lets the two shortcuts share one signature even
though one consumes args and the other doesn't:

| variant | meaning |
| --- | --- |
| `Returned(Expr)` | already closed the substitution; bubble out |
| `Continued(Statement)` | pushed a command stmt; loop continues |

Concrete helpers:

- `parse_status_word` — `$(status)` → `$?`, otherwise reconstruct as
  a `status` command with args parsed until the next pipeline / list
  / `)` terminator.
- `parse_getcpid_word` — `$(getcpid)` → `$!`, otherwise reconstruct
  as a no-arg `getcpid` command.

The `$(<file)` slurp form moved to `parse_slurp_form` (CC 3) so the
substitution loop only handles the general case.

## Result

| function | CC |
| --- | --: |
| `parse_command_substitution` | 22 → **16** |
| `parse_status_word` | 5 |
| `try_special_word` | 4 |
| `parse_slurp_form` | 3 |
| `parse_getcpid_word` | 2 |

The parent's remaining 16 = 1 (slurp early-return) + 1 (loop) + 1
(at_eof) + 3 (token-match arms) + 1 (stmts.is_empty) + 1 (if-let
Word) + 1 (`?`) + 3 (SpecialWord arms incl. None) + 4 (parse_statement
`?` + Some/None) — at the dispatcher floor for this shape.

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. `parse_status_word` and `parse_getcpid_word`
  produce byte-identical `Statement`/`Expr` shapes to the original
  inline blocks.
- The `SpecialWord` enum is a deliberately small local type — no
  module-wide impact, only `parse_command_substitution` and its two
  helpers reference it.
- Audit recipe suggested `Option<Expr>` as the return; the enum
  variant is needed because one path returns immediately and the
  other contributes a statement and continues. The enum keeps both
  cases in one function signature without resorting to a tuple of
  `(Option<Expr>, Option<Statement>)`.
