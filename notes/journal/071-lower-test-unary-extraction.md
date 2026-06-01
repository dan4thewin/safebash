# 071 — lower_test_unary extraction

## Goal

Reduce `codegen/tests.rs::lower_test_unary` (CC 32 live; audit 45) to
a flat op-dispatch shell.

## Approach

Six logical paths in the original:
- `-n` / `?` (implicit -n on bare word) — duplicated body
- `-z` — same shape with a different `IntCC`
- 20-entry file-test op table (`-e`, `-f`, …) with a nested match
- `-t` (atoi → isatty)
- `-v` (variable existence)
- unreachable

Extract:
1. `emit_len_cmp(builder, len, cc)` — single helper folds -n, -z, ?.
   Two arms share `IntCC::SignedGreaterThan`; -z uses `Equal`.
2. `file_test_op_code(op) -> Option<i64>` — pure 20-entry table. Returning
   `Option` lets the caller use the same arm for the test-vs-fallback
   distinction.
3. `emit_file_test`, `emit_isatty_test`, `emit_var_set_test` — the three
   non-trivial emitters.

The original `-v` arm already used the pre-extracted
`super::types::find_var_slot` (the audit's `find_highest_version_var()`
slot, done in an earlier wave). No new abstraction needed there.

Outer dispatch becomes:
```
match op {
    "-n" | "?" => emit_len_cmp(builder, len, SignedGreaterThan),
    "-z"       => emit_len_cmp(builder, len, Equal),
    "-t"       => emit_isatty_test(...),
    "-v"       => emit_var_set_test(...),
    other      => match file_test_op_code(other) {
        Some(code) => emit_file_test(..., code),
        None       => panic!("ICE: unimplemented unary test operator: {}", other),
    },
}
```

## Result

| function | CC |
| --- | --: |
| `lower_test_unary` | 32 → **8** |
| `file_test_op_code` | 20 (intrinsic; `cyclo-allow`'d) |
| `emit_var_set_test` | 5 |
| `emit_len_cmp` | 1 |
| `emit_file_test` | 1 |
| `emit_isatty_test` | 1 |

The CC-20 on `file_test_op_code` is the irreducible 20-entry table.
Tagged with a `// cyclo-allow:` comment alongside the existing
whitelist convention (cf. `expr_to_string`, the SsaInst/SsaValue
dispatchers in setup.rs).

## Verification

- `cargo build --release` — clean
- `cargo clippy --release --lib -- -D warnings` — clean
- `cargo test --lib --release` — 12/12
- `spec/run.sh` — 291/291

## Notes

- No behaviour change. The -n / ? arms had identical bodies in the
  original; this lift makes the equivalence explicit at the `|` pattern.
- `-e` and `-a` already shared op-code 0; `-L` and `-h` already shared
  op-code 7. Both kept inline in the helper since they're co-located
  with the rest of the table.
- The audit's CC-45 estimate for this fn was stale by two tiers — live
  was 32 before the rewrite. The cross-check-live-CC habit (added a
  few sessions back) continues to earn its keep.
