# 070 — expr_to_value dispatcher cleanup

## Goal

Reduce `transform/expressions.rs::expr_to_value` from a CC-34 monolith
to a 1-line-per-arm dispatcher matching the shape `expr_to_string`
sets next door.

## Approach

Extract one helper per non-trivial match arm. The audit explicitly
called for `lower_array_literal`; once that was done, the remaining
arms (`DoubleQuoted` all-literal collapse, `Concat`, `AssocArray`,
`Subscript`, `SimpleExpansion` EPOCHSECONDS, `ParameterExpansion`)
were each pushed into their own helper for consistency.

The `Array` arm splits further into two `Option`-returning probes
because the two patterns it recognises are genuinely independent:
- `try_array_regex_filter`: `(*  =~  'pat')` → `glob_regex`
- `try_array_single_glob`: `(*.sh)` → `glob`

Either fires, or the arm falls back to `IndexedArray(vals)`.

## Result

| function | CC |
| --- | --: |
| `expr_to_value` | 34 → **12** |
| `dq_to_value` | 7 |
| `lower_array_literal` | 4 |
| `try_array_regex_filter` | 4 |
| `try_array_single_glob` | 3 |
| `concat_to_value` | 3 |
| `simple_expansion_to_value` | 2 |
| `parameter_expansion_to_value` | 2 |
| `assoc_array_to_value` | 2 |
| `subscript_to_value` | 1 |

CC 12 is the floor for an 11-arm pure dispatcher (one CC per arm + 1).
Compare `expr_to_string` next door — same shape, CC 16, already
`cyclo-allow`'d as "pure ExprKind → String formatter". Same applies
here; if cyclo enforcement ever tightens past 12, this gets the same
allow comment.

## Verification

- `cargo build --release` — clean
- `cargo clippy --release --lib -- -D warnings` — clean
- `cargo test --lib --release` — 12/12
- `spec/run.sh` — 291/291

## Notes

- No behaviour change. Every helper is a 1:1 lift of an arm body.
- `simple_expansion_to_value` keeps the DD#40 EPOCHSECONDS coercion
  inline (the only other special-cased magic-variable name today;
  if more are added it splits into a table).
- `parameter_expansion_to_value` uses `let-else` to short-circuit
  the "no operator → just the var" case, matching the style used
  in `try_array_*`.
