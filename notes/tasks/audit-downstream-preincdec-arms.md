title: "Audit the remaining downstream `ArithExpr::PreIncDec` arms"
status: open
priority: P4
kind: cleanup
depends: []
gates:
  - "each remaining site is either proven reachable or removed"
  - "no spec flips"

DD#31 is categorical: no prefix `++`/`--`. `reject_prefix_incdec` enforces it
at **both** parse entry points — `parse_arith_slot` (the `(( … ))` statement
and all three C-style `for` slots) and `parse_arith_expansion` (`$(( … ))`).
So an `ArithExpr::PreIncDec` exists only long enough to be named in the
SB4020 message. Nothing downstream can see one.

Two arms were removed on that reasoning (journal 326): the `PreIncDec` arm of
`validate_arith_context`, and the `PreIncDec` arm of `arith_brief`. Both were
proven cold across eight and two spellings respectively, and both had an
independent type-level argument.

The rest of the cohort was **not** touched, because it spans three more
crates' worth of passes and each needs its own proof:

| site | what it is |
|---|---|
| `ir.rs:1133` | a `matches!` group |
| `parse/arith.rs:174` | `has_mutation_inner` |
| `parse/arith.rs:189` | `brief_arith` (the parser's own message — **live**) |
| `parse/arith.rs:239` | `classify_arith_expr` |
| `parse/arith.rs:1288` | `lower_arith_mutation_structured` |
| `transform/expr_handlers.rs:376` | |
| `transform/statements.rs:470` | |
| `validate.rs` (2 more) | `PostIncDec \| PreIncDec` groups |
| `emit_ir.rs:648` | `:arith-pre-incdec` in the IR dump |

Some are certainly live: `parse/arith.rs:189` is the message
`reject_prefix_incdec` itself prints, and `classify_arith_expr` may run on an
expression before rejection. The `PostIncDec | PreIncDec` groups cost nothing
to keep and removing the `PreIncDec` half changes no behaviour.

**Do not purge these as a batch.** The point of the campaign is that a cold
branch is not dead code until the spelling that would reach it has been looked
for and not found. Take them one at a time, with a probe.

`ArithExpr::PreIncDec` itself must stay: the parser constructs it so the error
message can name the variable.

## Found by

Branch coverage, 2026-09-15 — `validate.rs:1729` and `:2069`, both cold.
