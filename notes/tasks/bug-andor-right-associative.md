---
title: "BUG — && / || parse right-associative (false && A || B runs neither)"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "&& / || are left-associative: A && B || C parses as (A && B) || C"
  - "false && echo A || echo B -> B; [[ 1 = 2 ]] && echo A || echo B -> B"
  - "mixed/deep chains correct: A && B || C && D == ((A && B) || C) && D"
  - "no rust/spec regression (every && / || chain re-associates — verify the whole suite)"
---

> **Done in `4abe44a` (fix(parser): && / || are left-associative).**
> Verified 2026-06-21: `false && echo A || echo B` → B; `[[ 1 = 2 ]] && echo
> A || echo B` → B; `true && echo A || echo B && echo C` → "A C";
> `false || echo A && echo B` → "A B" — all match bash. The backend
> exit-status-vs-truthiness follow-up is the separate
> `bug-andor-backend-chain-status` (janet/c).


# BUG — `&&` / `||` parse right-associative

## Discovery

Found 2026-06-19 while verifying DD#62 F1. The DD#62 task flagged it as
"possibly a separate root cause" — confirmed: it is **not** about `[[ ]]`
operands, it is the `&&`/`||` list parser.

## Repro

```
$ sash -c '[[ 1 = 2 ]] && echo A || echo B'   # bash: B
(prints nothing)
$ sash -c 'false && echo A || echo B'          # bash: B
(prints nothing)
$ sash -c '[[ 1 = 1 ]] && echo A || echo B'    # A — happens to work
A
```

## Root cause

`parser.rs::binary_list_op` (≈1738) parses the right-hand side with
`parse_statement`, which recurses through `maybe_wrap_redirects` →
`parse_trailing_list_op` → `binary_list_op` again. So `A && B || C` is
built **right-associative** as `A && (B || C)`. When `A` is false, the
whole `(B || C)` is skipped → neither `B` nor `C` runs.

bash/POSIX: `&&` and `||` are **equal precedence, left-associative**, so
`A && B || C` = `(A && B) || C`.

(Note: codegen's `Terminator::Branch` ignores its `condition` field and
branches on the global `test_result_var` — the SSA lowering in
`transform/statements.rs::lower_list_stmt` is fine; the defect is purely
the parse tree shape.)

## Fix plan (left-associative, iterative)

A simple tree-rotation is **insufficient** — it mis-handles mixed deep
chains (`A && B || C && D` must be `((A && B) || C) && D`, not
`(A && B) || (C && D)`). Do it properly:

1. Extract the pipeline/redirect part of `maybe_wrap_redirects` (redirects
   + `parse_trailing_pipeline`) into a helper, separate from the list-op
   step.
2. Add a `parse_list_operand` that parses one command (dispatch + the
   pipeline part for non-compounds; bare compound for compounds, keeping
   `reject_compound_postfix`'s checks) and does **not** consume a trailing
   `&&`/`||`.
3. Rewrite `parse_trailing_list_op` as a **loop**: while the next token is
   `&&`/`||`, consume it, parse one `parse_list_operand`, and fold
   left: `left = List { left, op, right }`. `&` (Background) stays as-is.
4. `binary_list_op` folds into the loop (or becomes a thin helper).

Watch: compound right operands (`false && if …; fi`, `… && { …; }`,
`… && (subshell)`), `&` background precedence, and the `true`-default for
a malformed trailing operand. Verify the **entire** suite — this changes
the parse of every `&&`/`||` chain.

## Spec tests (write first)

- `false && echo A || echo B` → `B`
- `[[ 1 = 2 ]] && echo A || echo B` → `B`
- `[[ 1 = 1 ]] && echo A || echo B` → `A`
- `true || echo A && echo B` → `B`  (`(true||A)` skips A; `&& B` runs)
- `true && echo A && echo B` → `A\nB`
- mixed: `false && echo A || echo B && echo C` → `B\nC`
  (`((false&&A)||B)&&C` → B then C)

## References

- AGENTS.md "Bug policy"
- `sash/src/parse/parser.rs:1738` (binary_list_op), `:1624`
  (maybe_wrap_redirects), `:1729` (parse_trailing_list_op)
- DD#62 task section F (where this was surfaced)
