title: "DD#112 — a literal `arg` index must be a non-negative integer (SB5078)"
status: accepted

# DD#112 — a literal `arg` index must be a non-negative integer (SB5078)

## The rule

The index in `$(arg N)` / `$(arg N DEFAULT)`, **when it is a literal**, must be
a non-negative decimal integer. Anything else is a compile-time error, SB5078.

```sh
x=$(arg abc)     # SB5078
x=$(arg 1x)      # SB5078
x=$(arg -)       # SB5078
x=$(arg -x)      # SB5078
x=$(arg +1)      # SB5078
x=$(arg -1)      # SB5038 — still the DD#60 negative-index rule
x=$(arg 01)      # fine — digits, not spelling
x=$(arg $i)      # fine — computed, a runtime concern
```

## Why

The runtime parses the index by taking leading digits and ignoring the rest.
So a malformed literal did not fail — it **silently read a different
argument**:

| written | actually read | exit |
|---|---|---|
| `$(arg abc)` | argument 0 — the **program name** | 0 |
| `$(arg -)` | argument 0 | 0 |
| `$(arg -x)` | argument 0 | 0 |
| `$(arg 1x)` | argument 1 | 0 |
| `$(arg 2abc)` | argument 2 | 0 |
| `$(arg +1)` | argument 1 | 0 |

`$(arg 1x)` is the dangerous one: it looks like it worked, and for a
single-digit index it even returns the argument you probably meant. A typo in
the second digit changes which argument you get, with no signal at all.

This is not a new rule so much as an unfinished one. `arg_index_diag` already
existed as a typo guard, and DD#60 already states that `$(arg -1)` is rejected
at compile time. Spec `av.t6` already pins the governing intent for the
neighbouring case:

> `$(arg N)` for N >= argc aborts at runtime (no silent empty)

An out-of-range index refuses to guess. A malformed index was guessing.

## Scope: literals only

`is_nonnumeric_index_literal` returns `false` for anything that is not an
`ExprKind::Literal`, so `$(arg $i)` and `$(arg $(( n + 1 )))` are untouched. A
computed index is a runtime concern — the same split DD#60 already makes for
negative indices, and for the same reason: a literal is a typo you can see,
a computed value is not.

## Ordering

The new check runs **after** `is_negative_int_literal`, so `-1` and `-42` keep
SB5038 and its DD#60 wording. Only spellings that are not well-formed
negatives (`-`, `-x`) fall through to the broader rule. The existing specs
`av.t7` / `av.t7b` are unchanged and still pass.

## Notes

Found while probing `validate.rs:7485` for branch coverage — the branches of
`is_negative_int_literal` were cold because no spec exercised `$(arg -)` or
`$(arg -x)`, and reaching for inputs that would flip them is what surfaced
the silent read.

Specs `av.t8`–`av.t8i` in `spec/argv.sh`.
