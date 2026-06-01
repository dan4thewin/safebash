# 058 — extracting emit_command_kind from dispatch::emit

## Background

`SsaInst::emit` in `sash/src/codegen/dispatch.rs` was the
12th-most-complex function in the tree (CC 42, originally filed as
CC 63 before BuiltinCmd threading reduced it). Task `p3-emit`
asked for "convert to typed match, expected CC ~25".

Inspecting the function: the outer `match self` was already a
clean per-SsaInst dispatcher (each arm one call). The CC was
concentrated in a nested `match kind` inside `SsaInst::Command`
that handled ~17 `BuiltinCmd` variants — some inline, some
delegating to existing `emit_*` helpers. The inline arms with
internal conditionals (TrapSignal, SignalFlag, Noop/True/False,
RedirectPop, NegateTestResult) inflated the count.

## What changed

Extracted the entire inner `match kind` block from `emit`'s
Command arm into a new method `SsaInst::emit_command_kind` in
the same file. The Command arm in `emit` is now a single
delegation; the new method holds the full dispatch.

No behavior change. The `BuiltinCmd` arms remain byte-identical
to before — same inline bodies, same delegations to existing
`emit_*` / `handle_*` helpers, same fallthrough `_ =>
handle_general_command`.

The `redirect_stack` local in `emit` became dead after the
extraction; removed.

## Results

| function           | before | after |
| ------------------ | -----: | ----: |
| `emit`             |     42 |    15 |
| `emit_command_kind`|      — |    28 |

`emit` dropped out of the top-15 CC list entirely. The new
`emit_command_kind` is 28 — well above the threshold for the
report, but it's now a focused dispatch with no nested match
and one job (route a BuiltinCmd to its handler).

Total CC across the two methods is 43 vs. the original 42 — the
+1 is the cost of the new method's own dispatch entry. The win
is in per-function readability and reviewer load, not in summed
CC.

Verification:
- `cargo build --release`: clean
- `cargo clippy -D warnings`: clean
- `cargo test --lib`: 12/12
- spec: 291/291

## Why not further

Could have inlined-extracted the remaining inline arms in
`emit_command_kind` (TrapSignal, SignalFlag, Noop/True/False,
etc.) into their own per-arm `emit_*` methods. Decided not to:
each is 5–15 lines and the pattern is straightforward "iconst
some args, call some extern". Splitting them out would balloon
the file with one-call-site methods that obscure what each
BuiltinCmd actually does without reducing cognitive load.

The current split — outer dispatcher (15) + inner dispatcher
(28) — matches the natural two-level structure of the IR (SsaInst
→ BuiltinCmd) and keeps each method's "what does this do?"
answerable at a glance.

## Follow-up

`p3-emit-command` in the todo list points at `emit_command` in
the Janet backend (`sash/src/emit_janet.rs:510`, current CC 24,
originally filed as CC 39). That's a separate refactor —
unrelated to dispatch's emit.

The next p3 candidates by current CC (live, not as-filed):
- `handle_param_expansion` (CC 58) — `p3-handle-param-expansion`
- `lower_one` (CC 55) — already filed but not a p3 todo;
  need to confirm
- `handle_pipeline` (CC 54) — `p3-handle-pipeline`
- `lower_value` (CC 48) — `p3-lower-value`
- `print_arith_expr` (CC 48) — `p3-print-arith-expr`
- `to_tok` (CC 48) — logos-specific; review whether it's
  realistically reducible without losing the table-driven shape
