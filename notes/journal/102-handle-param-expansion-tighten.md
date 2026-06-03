# Journal 102 — handle_param_expansion routing tightened

## Goal

Third Phase 11 target. `handle_param_expansion`
(`codegen/value_handlers.rs:18`, CC 18). Recipe from the audit:
extract `load_var_or_empty` (collapsing a 3-arm where two branches
are bit-identical), pull the inline `${#arr[N]}` element-length
emitter out, factor the literal-intern step, and convert the
sequential `if operator == "…"` chain into a single match.

## Pattern

The body was an early-return ladder with three jobs interleaved:

1. **Glob short-circuit** (`glob`/`glob_regex`) — bypasses the var
   load entirely because the operand is the pattern, not the var.
2. **Value load** — three arms that all produced `(s_ptr, s_len)`,
   but two of them returned bit-identical `(null, zero)` with
   different commentary explaining why. Dead duplicate.
3. **Operator dispatch** — five `if operator == "…"` early-returns
   followed by an array-family `if op == "arrkeys" || …` followed
   by a fallthrough that interned the operand and called
   `emit_string_op_expansion`. Each branch hand-rolled its own
   cranelift code, including the 20-line `strlen_elem` block.

The CC counted every `if`, every `||`, and the nested let-else for
the value load.

## Action

Four helpers extracted:

| helper | purpose | CC |
| --- | --- | --: |
| `load_var_or_empty(var, vars, builder)` | merged the two `(null, zero)` arms; undefined and missing-but-defaulted both fall through cleanly | 2 |
| `emit_table_len(s_ptr, …)` | the `arrlen` body (one declare + call) | 1 |
| `emit_strlen_elem(operand, s_ptr, …)` | the 20-line table-lookup + itoa for `${#arr[N]}` | 1 |
| `intern_operand_pattern(operand, …) → (pat_ptr, pat_len)` | the 5-line module-intern dance for pattern operators | 2 |

With the helpers in place the body converts to a flat early-return
+ single match:

```rust
if matches!(operator, "glob" | "glob_regex") {
    let use_regex = operator == "glob_regex";
    return emit_glob_expansion(operand, use_regex, …);
}
let (s_ptr, s_len) = load_var_or_empty(var, vars, builder);
match operator {
    "strlen" => emit_int_to_str(builder, module, externs, s_len),
    "arrlen" => emit_int_to_str(builder, module, externs, emit_table_len(s_ptr, …)),
    "strlen_elem" => emit_strlen_elem(operand, s_ptr, …),
    "regex_replace" => emit_regex_replace_op(operand, s_ptr, s_len, …),
    op if op == "arrkeys" || op == "arrvals" || op == "arrjoin" || op.starts_with("arrvals_") =>
        emit_array_join_expansion(operator, operand, s_ptr, …),
    _ => {
        let (pat_ptr, pat_len) = intern_operand_pattern(operand, …);
        emit_string_op_expansion(operator, operand, s_ptr, s_len, pat_ptr, pat_len, …)
    }
}
```

The array-family arm still needs a `if-let` guard pattern because
`arrvals_<separator>` is matched by prefix; everything else is a
literal arm.

## Verification

```
handle_param_expansion    CC 18 → 11
load_var_or_empty                2  (new)
emit_strlen_elem                 1  (new)
emit_table_len                   1  (new)
intern_operand_pattern           2  (new)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

`handle_param_expansion` drops out of the top-15 entirely. The
remaining high-CC siblings in `value_handlers.rs` —
`emit_string_op_expansion` (20) and `emit_array_join_expansion` (15)
— are next in the Phase 11 queue (`p11-emit-string-op-expansion`).

## Note on the dead `(null, zero)` duplicate

The pre-refactor code distinguished "undefined variable with `:- /
- / :+ / +`" from "undefined variable, all other operators" and
emitted *identical* code for both. The comment claimed the first
branch was special-case handling, but it produced the same SSA. The
caller pipeline already deals correctly with the empty-string case
on the `:- / -` and `:+ / +` paths via `emit_string_op_expansion`,
so collapsing them is safe; all 291 specs and 12 lib tests confirm.
