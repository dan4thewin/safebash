# Journal 091 — scan_value_for_vars extract maybe_insert_positional

## Goal

First Phase 10 target. `scan_value_for_vars` (`codegen/values.rs`,
CC 29) — C-verdict from `r47-abstraction-audit.md:131`. Audit
recipe: "Pure variant traversal; multiple arms repeat positional-
name check. Extract `maybe_insert_positional`."

## Pattern

The positional/special-var predicate-and-insert appeared verbatim
4 times:

```rust
let name = &vref.name;   // or &var.name, or just `name`
if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
    refs.insert(name.clone());
}
```

Three sites in `scan_value_for_vars` (Var, ParamExpansion,
ArithExpr arms, all gated by `version == 0`) + one site in the
nested `scan_expr` helper inside `scan_test_for_vars`
(SimpleExpansion arm).

## Refactor

Extracted one helper:

```rust
fn maybe_insert_positional(name: &str, refs: &mut HashSet<String>) {
    if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
        refs.insert(name.to_string());
    }
}
```

Each callsite collapses from 4 lines (let + if + insert + clone) to
1 line (single call). Each saved 4 decision points (3 short-circuit
ORs + 1 if) in the parent.

## Result

| function | before | after |
| --- | --: | --: |
| `scan_value_for_vars` | 29 | 17 |
| `scan_test_for_vars` (sibling, only via embedded `scan_expr`) | 14 | 10 |
| `scan_expr` (nested) | 9 | 5 |
| `maybe_insert_positional` (new) | — | 5 |

Net: −15 CC across 3 functions, +5 CC for the new helper. Two
sibling functions (`scan_test_for_vars` and `scan_expr`) fell out
of the audit band entirely as a bonus.

## Insight

This is exactly the kind of cross-function pattern that the audit's
≥2-site rule was designed to catch — 4 sites, perfectly symmetric,
all in the same file. The fact that the predicate body is 4 short-
circuit ORs explains why CC bookkeeping shrinks dramatically per
site: each call deletes 4 decision points from the parent.

The combined predicate-and-insert helper (vs. splitting into
`is_positional_name` + `maybe_insert_positional`) is justified by
the only-here usage — no callers want just the predicate. If
either becomes wanted elsewhere, splitting later is trivial.

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/codegen/values.rs` — added `maybe_insert_positional`
  helper, collapsed 4 inline predicate-and-insert blocks.
- `notes/tasks/r47-abstraction-audit.md:131` — annotated.
