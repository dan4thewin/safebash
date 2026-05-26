title: Reduce parser-complexity hotspots (CC > 50)
status: ready
priority: P1
kind: work
source: opus-47-review
branch: logos-lexer
depends:
  - r47-decide-logos-vs-handroll
gates:
  - spec 285/285
  - cargo test all suites pass
  - mk lint clean
  - top function CC ≤ 50 after change
  - DD written if logos migration succeeds

## Direction (DG, 2026-05-26)

Logos for the fixed-token layer (decision D1). Work happens on a new
branch `logos-lexer` off `hand-parser`. If the migration is clean and
gates stay green, write a DD capturing the choice (next in sequence
after DD58).

## Current hotspots

`bash contrib/cyclo.sh sash/src --top 10` (2026-05-26):

```
88  tokenize_arith                sash/src/parse/arith.rs:295
78  next_token                    sash/src/parse/lexer.rs:320
70  intern_literals               sash/src/codegen/setup.rs:735
69  cli_main                      sash/src/lib.rs:280
68  scan_double_quoted            sash/src/parse/lexer.rs:953
63  emit                          sash/src/codegen/dispatch.rs:25
58  handle_param_expansion        sash/src/codegen/value_handlers.rs:18
57  lower_one                     sash/src/transform/statements.rs:27
54  parse_expr                    sash/src/parse/arith.rs:493
54  handle_pipeline               sash/src/transform/stmt_handlers.rs:264
```

## Targets in this task

Pick functions in this order; each should land in its own commit with
the spec/test gates green:

1. `tokenize_arith` (CC 88) — split the giant `match bytes[i]` by
   character class (numbers, dollar-prefixed, identifiers, operators,
   parens). Sub-helpers per class.
2. `next_token` (CC 78) — depends on D1. If logos lands, this becomes
   ~60% smaller automatically. If not, table-driven dispatch.
3. `scan_double_quoted` (CC 68) — extract the inner-token switch
   (`$var`, `${...}`, `$(...)`, `$((...))`) into per-form helpers.
4. `intern_literals` (CC 70) — split by `SsaValue` variant. Today this
   is one giant match per variant; per-variant inline-able fns are
   cheaper to read.
5. `cli_main` (CC 69) — extract arg parsing, --extract, --janet, --ssa,
   --ir paths into named functions.
6. `parse_expr` (in arith.rs, CC 54) — extract the Pratt prefix and
   infix tables into static data; reduce the dispatch to two helpers.

## Out of scope

- Anything already ≤50 — see `r47-set-extraction-floor-cc50`.
- Behavioral changes. This is pure restructuring; spec.db must stay
  285/285 after each commit.

## Why P1

These are the highest-leverage functions for code review and for the
adversarial-audit reviewers. They are also the densest source of
bugs-not-yet-found in any compiler. The `--top 10` list will not
change shape until these are addressed.

## Gate

`mk complexity` reports ≤50 for the top hotspot after the task is
done, and `mk check` is clean.
