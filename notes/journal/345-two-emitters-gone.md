# 345 — Two emitters, gone

The probe from 344 had already done the work; this is the part it was
actually for.

## The evidence

`emit_ir::ar_incdec` and `ar_subscript_assign` never fired across 5259
compiled programs. Two things made that mean something:

- **The mechanism could fire.** A control probe in `ar_assign` — which the
  allowed embedded assign `(( (a=2) ))` reaches — fired 27 times. An empty
  log from a broken writer proves nothing, and a gate that cannot fail is
  not a gate.
- **The sweep exercises the emitter.** `emit_ir` runs on *every* compile,
  not only under `--emit-ir`. So 5259 programs is the real denominator, not
  the handful of specs that pass the flag.

And the suite does contain the obvious sites: 25 C-style `for` headers and
two `a[i]` assignment statements.

## The structural argument

`emit_ir` renders the **post-validate AST**, so by the time it runs:

- `parse::lower_arith_command` has rewritten a top-level `(( a++ ))` or
  `(( a[i] += 5 ))` into a `Statement::Assignment`, and desugared a C-style
  `for` header the same way — no `ArithExpr` survives at all;
- `validate::check_arith_value_mutation` (SB4020) has rejected any mutation
  inside a `$(( … ))` value, whole-tree;
- `validate::check_arith_test_placement` (SB4020) has rejected `++`/`--` and
  `a[i]` mutations inside a `(( … ))` statement or condition, whole-tree.

That third gate is also why `ar_assign` is *live* while these two are not:
plain and compound assigns are the exception it allows.

The single route that bypassed all three was DD#93's attest carve-out —
which is what 344 was about, and is now closed.

## What was done

Both functions deleted. Both arms now ICE against a new
`ir::ARITH_IR_MUTATION_INVARIANT`.

They ICE rather than being dropped because `emit_ir::arith`'s match is
**exhaustive**: there is no permissive catch-all, so an arm cannot be
deleted, only replaced. That is the opposite of `hoist_mutation_node` in
343, where deletion was unsafe *because* a `_ => None` would have silently
swallowed the node. Same conclusion, two different reasons — worth keeping
straight, because the reason determines whether deletion is even an option.

Specs 104.6-104.8 pin the lowering, asserting the emitted capture holds
`:assignment` + `:arith-binop` and no `incdec` / `arith-subscript-assign`
node. A regression surfaces as an ICE at that spec rather than as a quietly
different capture.

## Ledger

- `emit_ir.rs` −45 lines. `emit_ir::arith` 17/1 — delta −16, inside the
  dispatcher exemption.
- Sweep **5288 passing, 13 skipped, 0 failing** (+3 native-only, predicted
  exactly). Units 83 green.
- `notes/tasks/dead-arith-mutation-emitters.md` closed. Its prediction that
  `hoist_mutation_node` was dead was wrong (343); its prediction that the
  three `ArithExpr` variants might be removable outright is disproved — the
  parser constructs them, and `CompoundAssign` is live.

## The thing the task file got right

It insisted on instrumentation over hand-picked probes. I had already run
six spellings by hand and concluded "dead"; the probe is what made that a
finding rather than a guess — and following it properly is what turned up
the four bugs in 344, none of which any of those six spellings touched.
