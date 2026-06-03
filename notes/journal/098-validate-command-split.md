# Journal 098 — validate_command per-rule extraction

## Goal

Eighth Phase 10 target. `validate_command`
(`validate.rs:198`, CC 19) — C-verdict from audit row 305. Recipe:
"Per-builtin SBxxxx error rules. Extract `check_heredoc`,
`check_dynexec`."

## Pattern

The function encoded six independent Bash-validator rules in one
70-line body. Each rule pushed a different SBxxxx diagnostic:

| rule | code | trigger |
| --- | --- | --- |
| heredoc as command name | SB5002 | `name == "__HEREDOC__"` |
| heredoc as arg | SB5002 | any arg Literal == "__HEREDOC__" |
| `export` rejected | SB5001 | `name == "export"` |
| `declare -x` rejected | SB5001 | `name == "declare"` + scan args |
| dynamic exec rejected | SB4001 | `name in {eval, source, ., command}` |
| coproc rejected | SB4001 | `name == "coproc"` |
| `[[ ]]` deep checks | SB5019/SB5023 | `name == "[[ "` + walk Tests |

The same 4-line `Diagnostic::error_span(span, "SB5002", "heredocs (<<…")`
text appeared twice (name-position + arg-position).

## Refactor

Five rule helpers + one diag constructor:

| helper | CC | rule(s) |
| --- | --: | --- |
| `heredoc_diag` | 1 | the duplicated SB5002 diag constructor |
| `check_heredoc` | 5 | SB5002 name + args; returns bool for short-circuit |
| `check_export_or_declare` | 7 | SB5001 export + declare -x |
| `check_dynexec` | 6 | SB4001 eval/source/./command + coproc |
| `check_test_constructs` | 4 | SB5019 + SB5023 inside `[[ ]]` |

The dispatcher is now 6 lines:

```rust
fn validate_command(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let name = expr_literal_str(&c.name);
    if check_heredoc(name, c, span, diags) {
        return;
    }
    check_export_or_declare(name, c, span, diags);
    check_dynexec(name, span, diags);
    check_test_constructs(name, c, span, diags);
}
```

Two design choices:
- `check_heredoc` returns `bool` rather than running unconditionally
  before falling through. The original returned early on either
  heredoc detection (parsing nothing further made sense; once
  there's a heredoc the rest is noise). Preserving the
  early-return shape avoids a behavior change.
- `check_dynexec` folds the `coproc` rule into the SB4001 family
  rather than spawning a one-off helper. Both rules emit SB4001;
  they were artificially split in the source.

## Result

| function | before | after |
| --- | --: | --: |
| `validate_command` | 19 | 2 |

The audit-band threshold was 15. All 5 helpers under it. Dispatcher
CC 2 (one `if-return`).

## Insight

This was the strictest example yet of "name every rule." The
original function was six rules pretending to be one function with
flow control between them. The rewrite makes the rule-set
inspectable at the dispatcher: a reader sees four rule-families
named by their semantic purpose, not by syntactic accident.

The `heredoc_diag` extraction is a 1-line helper that exists purely
to deduplicate a single error message. Without it the SB5002 text
lives in two places — easy to forget to update one. With it the
diagnostic is owned by a single function.

`check_heredoc`'s `-> bool` return value preserves the early-return
shape of the original. This is a low-risk way to keep behavioral
equivalence when extracting "stop processing on rule fire" logic.
Alternative would have been a unified `ControlFlow<()>` or
`Continue/Halt` enum — overkill for a single short-circuit point.

This closes the active Phase 10 queue. The remaining audit C/A
hotspots all need separate verdicts (e.g., `file_test_op_code` CC
20 noticed in journal 097, `scan_expr_positionals_expr` CC 21 now
visible in pre-commit top-5).

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/validate.rs` — dispatcher rewritten; 5 helpers added.
- `notes/tasks/r47-abstraction-audit.md:305` — annotated.
