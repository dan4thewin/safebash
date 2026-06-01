---
title: Audit string-keyed builtin dispatch outside BuiltinCmd::from_name
status: pending
depends: []
gates: []
---

# Audit string-keyed builtin dispatch outside BuiltinCmd::from_name

`BuiltinCmd::from_name` was introduced to centralize the
"is-this-a-builtin?" decision. **36 sites** still match builtin
names by string literal (`==` / `match`) outside the canonical
`from_name`/`sigil_str` entry points.

Snapshot 2026-05-31 from indexnudge `string_literals` view:

| file | sites |
|---|---|
| `sash-convert/src/convert.rs` | 3 |
| `sash/src/codegen/entry.rs` | 1 |
| `sash/src/codegen/inst_handlers.rs` | 9 |
| `sash/src/codegen/value_handlers.rs` | 2 |
| `sash/src/emit_janet.rs` | 2 |
| `sash/src/parse/lexer.rs` | 1 |
| `sash/src/ssa_taint.rs` | 4 |
| `sash/src/transform/control_flow.rs` | 1 |
| `sash/src/transform/resolution.rs` | 4 |
| `sash/src/transform/stmt_handlers.rs` | 2 |
| `sash/src/validate.rs` | 7 |

## What each site needs

For each, decide one of:

1. **Replace with `BuiltinCmd::from_name`** — most likely for
   dispatch in `inst_handlers`, `resolution`, `stmt_handlers`,
   `value_handlers`. Several already match the literal sigils
   the enum knows about.
2. **Keep as-is with a justification comment** — for sites where
   the literal isn't really "is this a builtin?" but rather "is
   this *this specific* builtin?" (e.g. `ssa_taint`
   distinguishing `getenv`/`argv` from other builtins for taint
   classification). These should still gain a comment so a future
   reader doesn't assume they slipped through.
3. **Move into a new `BuiltinCmd` method** — if the same predicate
   shows up in 2+ places, it belongs on the enum. Likely
   candidates: `is_taint_source` (currently inline in
   `validate.rs:1215` and similar code in `ssa_taint.rs:247`),
   `is_pipeline_inline_builtin` (4 sites in `resolution.rs:11`,
   probably belongs as `BuiltinCmd::runs_inline_in_pipeline`).

## Reproduce the audit

```bash
sqlite3 .indexnudge-rust.db "
  SELECT file, line_num, symbol, value, context
  FROM string_literals
  WHERE value IN ('echo','true','false',':','printf','exec','trap',
                  'read','bless','getenv','argv')
    AND context IN ('compare','match_pat')
    AND (symbol IS NULL OR symbol NOT LIKE '%from_name%')
  ORDER BY file, line_num;"
```

After each change, re-run the same query; the count should only
decrease (or rationale comments should appear at the surviving
lines).

## Why now

`BuiltinCmd::from_name` was introduced specifically to make this
class of dispatch checkable. Until we sweep, the abstraction is
half-installed: new readers can't tell whether a `name == "echo"`
is legitimate or a missed cleanup.

## Discovered

2026-05-31, via indexnudge v7 `string_literals` view (see
`feature-request-string-literal-index.md` filed earlier — this is
the exact use case it was filed for).
