# 326 — a rule enforced twice is enforced once

Two more cold arms in `validate.rs`, both `ArithExpr::PreIncDec`: one in
`validate_arith_context` (the SB5007 "prefix ++x is not allowed" message),
one in `arith_brief` (how a prefix form would be spelled back to the user).

Eight spellings aimed at the first — statement, expansion, condition, all
three `for` slots, a comma element, a parenthesised form, a ternary arm, an
assignment RHS. All eight stayed cold. Two at the second, same.

That is enough to suspect, not enough to delete. The argument that settles it
is structural, and it comes in two independent halves:

1. `reject_prefix_incdec` runs in **both** parse entry points —
   `parse_arith_slot` (the `(( … ))` statement and all three C-style `for`
   slots) and `parse_arith_expansion` (`$(( … ))`). Every prefix form is an
   SB4020 before validation begins.
2. Even granting a leak past that, a top-level `PreIncDec` classifies as a
   mutation, so `lower_arith_mutation_structured` builds an `Assignment` —
   never the `ArithTest` that is `validate_arith_context`'s only caller.

And for `arith_brief`: it recurses only through `BinOp` operands, and
`reject_prefix_incdec` recurses there too.

Both deleted. `arith_brief` has a `_ => "..."` fallback, so the worst case if
the reasoning were wrong is a vaguer message — not a crash and not a missed
error.

## The thing worth being uneasy about

Deleting these leaves `reject_prefix_incdec` as the **single** enforcement
point for a DD#31 clause. That is thinner than it was.

The answer is not to keep a duplicate warm. A second check that no input can
reach does not enforce anything; it only makes the code look defended. What
actually defends the rule is a spec, and the parser's rejection is now pinned
at all four entry points: `cov.23` for `(( ++x ))`, `cov.163`–`165` for the
three `for` slots, and `cov.176` (new) for `$(( ++x ))`. `31.6` covers the
statement form independently.

That is the trade the campaign keeps making: replace unreachable code with a
test that runs.

## The rest of the cohort, deliberately untouched

Nine more `PreIncDec` sites across `ir.rs`, `parse/arith.rs`, `transform/`,
and `emit_ir.rs`. Some are certainly live — `brief_arith` at
`parse/arith.rs:189` is the message `reject_prefix_incdec` itself prints.
Filed as `audit-downstream-preincdec-arms` (P4), explicitly **not** as a batch
purge. A cold branch is not dead code until the spelling that would reach it
has been looked for and not found, and that is a per-site question.

## Also closed this pass

Seven more `validate.rs` branches, none of them bugs — just spellings nobody
had written:

| branch | spelling |
|---|---|
| `check_for_word` guard, both arms | `for x in $@` / `for x in $*` → SB5045 |
| `check_glob_chars_in_literal`, `[` arm | `for x in a[bc]d` → SB5006 |
| …its `]` half, false | `for x in a[bc` — an unbalanced bracket is a literal |
| …its `[` half, false | `for x in abc` |
| `check_unquoted_glob` `Concat` arm | `for w in pre$s*post` |
| `arith_mutation_under_cond` ternary, both operands | `if (( 1 ? (x=2) : 3 ))` and `if (( 1 ? 3 : (x=2) ))` |

The ternary pair is the one to notice: the two arms are the two operands of a
single `||`, so covering one leaves the other cold. Short-circuit operators
hide a branch behind a branch, and a spec that exercises "the ternary case"
reaches exactly half of it.

Specs `cov.168`–`cov.176`. Sweep **4891 passing, 10 skipped, 0 failing**.
