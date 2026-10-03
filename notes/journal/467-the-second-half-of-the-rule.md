# 467 — the second half of the rule

DG's policy, from journal 466:

> For any given function, I want every line warm by spec coverage except
> ICEs first. Then I want a unit test to cover the final ICE lines.

466 did the first half for the reachable cohort. This session did the second:
**28 ICE lines, 24 new unit tests, across eleven files.** The lib suite goes
71 → 99.

## What an ICE test is for

The ICE is the claim. `unreachable!("SB4045 rejects a zero-argument argv")`
asserts something about a *different pass*, and nothing in the type system
holds it. A unit test calling the lowering directly with the shape SB4045
rejects is the only artefact that can fail when the validator develops a
hole — the spec suite cannot, because the spec suite can only write programs
the validator accepts.

So each test reads as a sentence: *"this shape cannot get here, and here is
what happens if it ever does."* A test that stops panicking is the finding.

## What got covered

| file | sites | guarded by |
|---|---|---|
| `ir.rs` | `children`, `mutation_target_in`, `arith_brief` | `PREFIX_INCDEC_INVARIANT` |
| `parse/arith.rs` | `classify_arith_expr`, `lower_arith_mutation_structured`, `tok_to_text` | same, + DD#116 rival |
| `parse/expr.rs` | `parse_quoted_arm`, `parse_expansion_arm` | the word loop's own routing |
| `parse/parser.rs` | `build_expr_from_parts` | an arithmetic argument about two call sites |
| `validate.rs` | `arith_has_incdec`, `check_positionals_arith` | `PREFIX_INCDEC_INVARIANT` |
| `emit_ir.rs` | `expr_composite`, `arith` ×3 | `expr()`'s split, `ARITH_IR_MUTATION_INVARIANT` |
| `transform/statements.rs` | `lower_one`, `hoist_mutation_node` ×2 | `push_pipeline_stage`, SB4020 |
| `transform/expressions.rs` | `composite_to_value` | `expr_to_value`'s routing |
| `transform/cmd_handlers.rs` | `handle_signal` | SB4062 |
| `transform/expr_handlers.rs` | `join_intrinsic` ×2, `arith_to_ssa` | SB5074, SB5040, SB4020 |
| `transform/stmt_handlers.rs` | `handle_attest_stmt`, `handle_argv` ×3, `handle_keys` | SB4014, SB4045, SB5085, SB5073 |

Seven of the twenty-eight are the same shape — `ArithExpr::PreIncDec`, which
`reject_prefix_incdec` raises SB4020 on at both arith entry points. One
invariant, seven walkers that each have to say so. That is the cheapest kind
of coverage there is, and it is also the one most likely to rot: nothing
connects the seven to each other except the shared `const`.

Verified the way 466's specs were: an instrumented `cargo llvm-cov --lib` run,
then the 28 line numbers checked against the lcov by hand. All 28 warm; 32
lines total, because rustfmt spreads four of the `panic!`s over a second line.
513 cold → 481 (and ~467 once `coverage.db` catches up with 466's specs).

## Three ICE arms a unit test cannot reach either

The third category this campaign keeps producing. A guard is unreachable
because an *earlier pass* rejects the shape — fine, a unit test calls past the
pass. But some are unreachable because the **same function** already covered
the case, and then nothing can call them.

- `transform/expr_handlers.rs::arith_to_ssa`'s trailing arm. The match names
  twelve `ArithExpr` variants; `arith_leaf_to_ssa`, called first, returns
  `Some` unconditionally for the other four. Sixteen of sixteen. It was
  spelled `_ =>`, so a new variant would have landed on the ICE at runtime
  rather than failing the build — exactly the non-total-walker shape that hid
  the bugs in journals 430, 432 and 456. **Changed to name the four leaves.**
  The line stays cold; the compile-error property is what it buys.
- `transform/stmt_handlers.rs::handle_attest_stmt`'s `ArithConvert` arm. A
  DD#116 property-2 totality arm, already documented as one. Left alone.
- `parse/expr.rs:260`, `parser.rs:201/207/2424/2438/2987` — not examined this
  session; several are `panic!` with format arguments and may be reachable.

## Still open

The ~19 **validate-shadowed** arms from 466 are still DG's call: make them
ICEs (and then unit-test them under this same rule), delete them, or annotate
them deliberate. Nothing changed.

The harder ICE cohort is untouched: `emit_c.rs` ×19, `codegen/values.rs` ×7,
`codegen/types.rs::find_array_slot`, `codegen/dispatch.rs::emit_match`,
`codegen/value_handlers.rs::emit_string_op_expansion`. Those need an `Emit`
buffer or a Cranelift `FunctionBuilder` fixture, which is a bigger piece of
scaffolding than everything above put together, and worth doing once rather
than per-site.
