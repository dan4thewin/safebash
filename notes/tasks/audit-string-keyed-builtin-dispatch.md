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

## Re-measured 2026-09-12 — much larger than filed

The filing's 2026-05-31 snapshot counted **36** sites. A comparable query over
the current tree (builtin/intrinsic names matched by `==` or `match` pattern,
excluding test files and the canonical `from_name` / `sigil_str` entry
points) returns **153**, with `validate.rs` alone holding **69**.

| file | sites |
|---|---|
| `sash/src/validate.rs` | 69 |
| `sash/src/emit_janet.rs` | 13 |
| `sash/src/type_check.rs` | 11 |
| `sash-convert/src/convert.rs` | 9 |
| `sash/src/emit_janet_map.rs` | 8 |
| `sash/src/ssa_taint.rs` | 7 |
| (13 more files) | ≤ 4 each |

Whatever the exact overlap with the original methodology, the surface has
grown several-fold rather than shrunk. Scope this against the current number
before committing to a sweep.

## Re-measured 2026-10-13 — 57 sites, and the 153 was inflated by deleted files

The 2026-09-12 note asked for scoping before a sweep and nobody did it, so this
sat out of the queue on its own gate. Discharged now.

**The count is 57, not 153.** That figure came from a broader query that
included `emit_janet.rs` (13) and `emit_janet_map.rs` (8) — both **deleted by
DD#104**. It also counted argument-position strings, not just dispatch.
Re-measured against `string_literals` with `context IN ('compare','match_pat')`:

| file | sites |
|---|---|
| `sash/src/validate.rs` | 25 |
| `sash-convert/src/convert.rs` | 11 |
| `sash/src/type_check.rs` | 3 |
| `sash/src/ssa_taint.rs` | 3 |
| `sash/src/codegen/inst_handlers.rs` | 3 |
| (rest) | ≤ 2 each |

**Most of it is not worth sweeping.** The bulk are the task's own category 2 —
"is this *this specific* builtin?" (`is_getenv_lookup` and friends). Routing
those through `from_name` would convert a string compare into an enum compare
and change nothing about correctness, since there is one name on each side.

**The part that is worth doing is category 3**, and inspecting the 25
`validate.rs` sites turned it up: **six** functions answer the same question —
"which argument of this command is the out-target?" — from six hand-written
name lists.

| line | function | names |
|---|---|---|
| 5301 | `command_writes_args` | read/readline, match/matchall, argv, keys |
| 6847 | `taint_command` | read/readline/**mapfile**/argv |
| 7635 | `collect_read_targets` | read/readline, matchall/match, argv |
| 8572 | `insert_auto_collection_target` | read (**no readline**), match/matchall, argv, keys |
| 9283 | `bulk_fill_targets` | read/readline/**mapfile**/argv |
| 9556 | `fill_target_spec` | argv, keys, match/matchall |

**Checked, and the divergences are correct, not bugs** — which is the useful
result, because the shape is the one that has produced six bugs already (see
the "statement wrappers" and "shared walkers" findings):

- `readline` is absent from `insert_auto_collection_target` **by design**:
  DD#99 says "`readline` binds a scalar", so it never auto-creates a
  collection.
- `mapfile` is **vestigial**. DD#99 stage 6 retired it; `retired_bulk_read_diag`
  (`validate.rs:10962`) raises SB5060 for `mapfile`/`readarray` before any of
  this matters. The alternative is almost certainly unreachable — stated as a
  hypothesis, since line coverage cannot see *which* alternative of a
  `matches!` fired, and nobody has branch-probed it.

So the honest scope is not 57 sites. It is: delete the dead `mapfile`
alternatives (3 sites) once branch-probed, and decide whether the six
out-target predicates should become one shared function with a documented
question. The latter is the `types::var_slot` lesson (journal 506) applied
again — six copies of one question is six chances for a fix to land on five.

## Recommendation

**Re-file narrowly** as "collapse the six out-target predicates in validate.rs"
and drop the 57-site sweep. P4 — it is a latent hazard, not a live fault.
