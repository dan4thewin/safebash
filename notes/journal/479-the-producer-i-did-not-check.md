# 479 — the producer I did not check

Coverage queue. Four candidates, three outcomes, and one wrong answer that the
spec suite caught within a minute of my writing it.

## `emit_degenerate_pipeline` — unreachable, and deleted

`emit_pipeline` opened with a guard for a 0- or 1-stage pipeline: return for
zero, and for one, emit it as a plain command instead of fork/pipe machinery.
Both branches cold.

Tracing it back: `lower_pipeline_body` routes `p.commands.len() > 1` to the
stage builders and sends a lone command to `lower_one` instead, and every stage
pusher — `push_command_stage`, and the compound arm of
`lower_one_pipeline_stage` — pushes exactly one stage per source command. So a
`SsaInst::Pipeline` cannot carry fewer than two stages.

The native backend agrees, by omission: `handle_pipeline` computes the *same*
`num_pipes = if capture_read_fd { n } else { n - 1 }` with no guard at all. On
a `usize`, `n - 1` at zero wraps, so native would have asked for `usize::MAX`
pipes where C quietly rewrote the program.

That asymmetry is the argument for deleting rather than keeping. `emit_c` is
the differential oracle — the whole reason the spec suite runs twice is that a
divergence localises a bug to one backend. A defensive branch in the oracle
that silently normalises an input the native backend mishandles is the oracle
absorbing the difference it exists to report.

So: `emit_degenerate_pipeline` deleted, and the shared expression moved to
`ssa::pipeline_num_pipes`, which both backends now call and which ICEs on a
degenerate count.

### Which is where I got it wrong

I wrote the floor as `n > 1` for both forms. `mk test` passed. `cargo clippy`
passed. `spec/taint.sh` failed fifteen checks, the first being `24.6`:

```
echo hello | readline x; echo $x
```

A capture pipeline — `cmd | readline x`, `cmd | while readline …` — does not
put every command in `stages`. It takes `&p.commands[..p.commands.len() - 1]`,
the **producers**, and leaves the reader to the parent. Two commands, one
stage. `pipeline_num_pipes(1, true)` is not a bug, it is the common case.

I had traced exactly one of the three producers of `SsaInst::Pipeline`
(`handle_regular_pipeline`) and generalised from it. The other two —
`handle_pipe_while_or_array_read` and `handle_lastpipe_read` — count something
different. The deleted guard had known this: its condition was `n == 1 &&
!capture_read_fd`, and the `!capture_read_fd` was carrying the whole fact.

Journal 439 said the same thing about a different walker, in the same words:
*the enumeration must be of the producer, not the transform you think removes
it*. Three producers, and checking one of them is checking none of them.

The floor is now per-form, and the unit test says why out loud so the next
person does not re-derive it from a fifteen-check failure.

A second, smaller trap on the way out: `assert!(cond, "literal")` panics with a
`&'static str`, while `assert!(cond, "fmt {n}")` panics with a `String`. The
test downcast to `String` and so *failed on the branch that was working
correctly*. Both messages now carry their count.

## The five unlowerable values — stated once instead of mirrored

`codegen::values::lower_value` and `emit_c::lower_value_as_str_aggregate` each
spelled out five ICE arms: `FuncCall`, `IntFuncCall`, `CmdSubstBody` (lifted
during transform), `ArraySplat`, `ElideEmpty` (expanded by a list builder). Ten
arms, five claims, maintained by hand. The C side even said so:

> Native ICEs on exactly these (codegen/values.rs:81-115); c mirrors it arm for
> arm.

Those arms were at 70-120. The cross-reference had already drifted off the
thing it named, which is the whole case against hand-mirroring in one line of
evidence.

All five now route to `ssa::unlowerable_scalar_value(backend, value)`, one arm
per backend. The claims are **orthogonal** backstops in DG's sense and so they
stay: `validate` and `transform` decide these questions about the AST, and this
is the only site that can say anything about the lowered `SsaValue` that came
out the other side. A lifting pass that missed a case is invisible upstream.

Pinned by two unit tests. The first checks each of the five names *its own*
claim, not merely that something panicked — an arm falling through to a
neighbour would still panic and would still look fine. The second covers the
new seam the consolidation creates: the callers select the five by variant and
the helper spells them out again, so if those two lists drift, a lowerable
value arrives at a function whose job is to refuse. It reports the drift rather
than mislabelling it as a refusal.

## `strip_condition_negation` — the second destructuring, removed

Three cold `return`s, and the doc comment said why: *the shape is already
established by `negated_attest_condition`, so each `else` here is unreachable.*

Two functions, one shape, split only by `&` versus `&mut`: a predicate that
tests for `if ! attest …` and a rewriter that strips the negation. An ICE in
the rewriter would have been DG's bad case exactly — re-asking the predicate's
question on the predicate's data. The right answer is not to backstop the
duplicate but to not have it.

They are now one function, `take_negated_attest`, which tests and strips in one
pass and has one early-exit path instead of two sets. Both had a single caller,
and the same caller.

Worth noting what the cold lines were hiding, because it is not nothing: a
`return` in the rewriter leaves the negation in place, so `if ! attest … { A }
else { B }` keeps its branches unswapped and the compiled program means the
opposite of what the source says. Silent, and inverted.

## `sig_scan_test` — a genuine gap, on an everyday idiom

`TestExpr::Not`, `And` and `Or` all cold. Only the `Unary`/`Binary` leaves had
ever run, which means no function signature scan in 7801 checks had seen

```
fn f { [[ ${args[0]} == x && ${args[1]} == y ]] && echo both }
```

The arms are live and correct — verified by execution before writing anything:
`&&` records both positionals, `!` records its operand, and a short call is
SB5042. But correct-and-unexercised is a claim, not a fact, and the failure
mode is quiet. An arm that failed to recurse would not crash; it would
*under-count* the arity, and a call omitting the argument would compile and
read an argument nobody passed.

Four specs, `81.33`-`81.36`. `81.33` and `81.35` are the diagnostics; `81.34`
is the control, because `81.33` alone is satisfied by a signature that
over-counts — the pair is what pins the index set rather than just its size.
`81.36` puts a `!` inside an `||` so both recursive arms run on one test.

## Gate

`mk test` 85 + 27 + 20 (was 81). Sweep **7809**, predicted before running
(7801 + 2×4). 0 failing, 0 flips, exactly the eight ids I added, regression
self-test 248.

## Left on the queue

- `lower_value_as_str_aggregate`'s `other =>` arm. It ICEs at runtime on a new
  `SsaValue` variant, which satisfies the "a new variant must not be silently
  accepted" rule, but it does so at runtime rather than as a compile error.
  Making it total costs naming the ~16 variants `lower_value_as_str` and
  `lower_string_transform_c` own, in a split that exists only to satisfy the
  complexity gate. Worth doing; not obviously worth doing *here*.
- `take_end_block`, `parse_trailing_pipeline`, `argv_splat_start`,
  `compile_native`'s diagnostic loop, `walk_arith_subscripts`'s
  `SubscriptAssign` arm, `emit_attest` / `load_var_or_empty` /
  `parse_var_base_version`'s fallbacks — unexamined.
- `insert_case_phis`'s single-path arm and `sysroot_file_exists`'s `.squashfs`
  branch, both still DG's call (status 2026-10-08).
