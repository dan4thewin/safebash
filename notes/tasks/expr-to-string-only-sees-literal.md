title: expr_to_string only ever sees ExprKind::Literal — decide what it is
status: open
priority: P3
kind: task
depends: []
gates:
  - "every call site of `expr_to_string` is enumerated, with the ExprKind each can pass"
  - "each cold arm is classified: spec gap, validate-shadowed, or structurally unreachable"
  - "`joined_form_to_string` / `opaque_form_to_string` either become reachable or say why they are not"

# `expr_to_string` only ever sees `ExprKind::Literal`

Measured at `c6182e4` over a full union coverage run: `expr_to_string`
(`sash/src/transform/expressions.rs:484`) is entered **21225 times** and the
`Literal` arm is the only one ever taken. Every other arm is cold:

    487  SimpleExpansion       0
    488  SingleQuoted          0
    490  ParameterExpansion    0
    492  SlurpFile             0
    494  Subscript             0
    499  -> joined_form_to_string   0
    501  -> opaque_form_to_string   0

Both helpers show `count 1` at their entry line, which is the in-crate ICE
unit test calling them with a form they do not own — not a production call.
So `joined_form_to_string` (Concat / DoubleQuoted / Array / AssocArray) and
`opaque_form_to_string` (CommandSubstitution / Arithmetic / Test) have never
run for real.

That is 21 cold lines across the three functions, and it was the second-,
third- and eleventh-largest entry in the per-function cold ranking.

## What this is not

The obvious candidate was a `${var:-…}` operand: `pe_operand_string`
(:368) renders one with `expr_to_string`, and it reaches that call 33 times.
A double-quoted operand looked like it would route to
`joined_form_to_string`'s `DoubleQuoted` arm.

It does not. `spec/var_modifiers.sh` `vm.dv12` was written for exactly that
spelling — `echo "${foo:-"x"}"` yields `"x"`, quotes and all, per DD#65 — and
measured afterwards, line 513 is **still cold**. DD#65's 2026-09-30 follow-on
is why: the operand stopped being an `ExprKind::DoubleQuoted` and became a
`Literal` carrying the raw body text. The spec is worth keeping on its own
merits (it was the one `${var:-…}` spelling with no coverage), but it is not
the route in.

## What to do

Enumerate the call sites, not the behaviour of samples. There are 24:

    expressions.rs:121   operand_pattern_string   (its own `_ =>` fallthrough)
    expressions.rs:226   fill_target_name
    expressions.rs:371   pe_operand_string
    expressions.rs:492   expr_to_string           (SlurpFile recursion)
    expressions.rs:494   expr_to_string           (Subscript index recursion)
    expressions.rs:522   joined_form_to_string
    cmd_handlers.rs:21   handle_read
    cmd_handlers.rs:88   handle_signal
    cmd_handlers.rs:91   handle_signal
    control_flow.rs:372  lower_if
    control_flow.rs:623  argv_splat_start
    control_flow.rs:730  detect_array_iter
    statements.rs:108    try_producer_tail_call
    statements.rs:267    body_calls_user_function
    statements.rs:736    is_attest_left
    stmt_handlers.rs:28  handle_command
    stmt_handlers.rs:565 handle_keys
    stmt_handlers.rs:566 handle_keys
    stmt_handlers.rs:700 is_array_read_last
    stmt_handlers.rs:708 is_lastpipe_read_last
    stmt_handlers.rs:754 push_command_stage
    expr_handlers.rs:38  cmdsub_body_value
    expr_handlers.rs:438 collection_operand_name

Most look like *name* extraction — a command head, a fill target, a collection
operand — which `validate` guarantees is a `Literal`. If that holds for all of
them, the function has outgrown its signature: it is `literal_name_of()` with a
total `match` bolted on, and the honest outcomes are either to narrow it or to
make the arms ICE with the guarantee named (per the repo's
"unreachable by construction → `unreachable!()`" rule).

`operand_pattern_string:121` is the one site that plausibly passes something
else — its doc says it handles `SingleQuoted`/`DoubleQuoted`/`Concat` itself
and delegates "everything else", which would include `SimpleExpansion`.

**Closed, 2026-10-10 — it cannot.** Both callers (`test_to_ssa:149` and
`dispatch_test:946`) guard the call with `!expr_is_dynamic(...)`, and
`expr_is_dynamic` answers `false` for exactly `Literal`, `SingleQuoted`, and a
`DoubleQuoted`/`Concat` whose parts are all non-dynamic — every other kind is
`true` via its `_ =>` arm. The three non-`Literal` survivors are taken by
`operand_pattern_string`'s own arms, so the delegation receives `Literal` and
nothing else. By construction, not by sampling.

That was the likeliest of the 24 to pass something else, which strengthens the
"outgrown its signature" reading considerably. 23 sites left to enumerate.

Do **not** conclude from coverage alone. Journal 439 is the standing warning:
checking the transform you think removes a form said "unreachable", checking
the *producer* said "reachable", and three of eight candidates failed that way.
