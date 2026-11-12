title: "Purge the `PreIncDec` dead node (or decide it stays)"
status: done
priority: P3

# Purge the `PreIncDec` dead node (or decide it stays)

`ArithExpr::PreIncDec` is constructed by `parse_pre_incdec`
(`sash/src/parse/arith.rs:1116`) and then **always rejected before it
leaves the parser**. It exists only so `reject_prefix_incdec` can name
the offending variable in its diagnostic.

## Evidence

`reject_prefix_incdec` is called at both arithmetic entry points, ahead
of every other check:

- `parse_arith_expansion:78` — `$(( … ))`
- `parse_arith_slot:280` — `(( … ))` and all three C-style `for` slots

and it recurses through `BinOp`, `Assign`, `CompoundAssign`, `Comma`
and `Paren` — the same node set as `has_mutation_inner`, so nothing can
hide a `PreIncDec` below it.

The two `parse_arith_str` callers that bypass `parse_arith_slot`
(`expr.rs::emit_string_part:508`, `arith.rs::parse_expansion_atom:1048`)
carry their own rejection. Probed:

| input | result |
|---|---|
| `echo $(( ++y ))` | SB4020 prefix |
| `echo $(( 1 + ++y ))` | SB4020 prefix |
| `(( ++y ))` | SB4020 prefix |
| `(( y = 1 + ++y ))` | SB4020 prefix |
| `echo $(( (++y) ))` | SB4020 prefix |
| `echo "$(( ++y ))"` | SB4020 (string path) |
| `echo "${y:-$(( ++y ))}"` | SB4020 (string path) |
| `for (( i=0; i<2; ++i ))` | SB4020 prefix |

## The unreachable handling

- `parse/arith.rs:189` — `brief_arith`'s `PreIncDec` arm (2 cold
  branches; this is what surfaced it)
- `parse/arith.rs:1292`, `ir.rs:1133`, `validate.rs:2033`,
  `validate.rs:3442` — shared `PostIncDec | PreIncDec` patterns where
  only the `PostIncDec` alternative can fire
- `transform/statements.rs:470`, `transform/expr_handlers.rs:376`
- `emit_ir.rs:648`

## The question for DG

Deleting the arms is mechanical and git makes it reversible, but it
spans four passes, and a parse-only IR node keeping defensive
downstream arms is a defensible position: if a future change ever lets
one through, an `unreachable!()` is a better outcome than a silently
wrong lowering.

Three options:

1. **Delete** every `PreIncDec` arm downstream of the parser.
2. **Convert** them to `unreachable!()` with the invariant named — the
   `join_intrinsic` convention already in the tree.
3. **Leave** them and accept ~10 permanently cold branches.

Option 2 keeps 100% branch coverage honest (an `unreachable!()` is a
should-never-run assertion, the kind `mk cov-unit` is deliberately off
to avoid hiding) while documenting *why* the code cannot run.

Found in journal 338.

## Measured 2026-09-16 (journal 341) — the filing over-counts by 5x

**Do not execute as filed.** The invariant is real: all ten spellings of a
prefix `++`/`--` reject with SB4020, covering both arith entry points
(`parse_arith_expansion`, `parse_arith_slot` incl. all three C-style `for`
slots) and both `parse_arith_str` bypass callers (`emit_string_part`,
`parse_expansion_atom`). `PreIncDec` never survives the parser.

But "~10 cold branches" is wrong. Measured against `coverage.db`:

| site | function | catch-all | line count | delete buys |
|---|---|---|---|---|
| arith.rs:189 | `brief_arith` | `_ => "..."` | **0** | 1 cold line |
| emit_ir.rs:648 | `arith` | none (exhaustive) | **0** | 1 cold line |
| expr_handlers.rs:376 | `arith_to_ssa` | `_ =>` | **0** (body 380) | nothing — already `unreachable!()` |
| arith.rs:174 | `has_mutation_inner` | `_ => false` | arm hot (176: 4) | nothing |
| arith.rs:239 | `classify_arith_expr` | `_ =>` | arm hot (241: 468) | nothing |
| arith.rs:1292 | `lower_arith_mutation_structured` | `_ =>` | arm hot (1293: 119) | nothing |
| ir.rs:1133 | `arith_has_mutation` | none | arm hot (1134: 63) | nothing |
| validate.rs:2033 | `arith_has_incdec` | none (`matches!`) | arm hot (2034: 712) | nothing |
| validate.rs:3442 | `check_positionals_arith` | none | arm hot (**1305**) | nothing |
| statements.rs:482 | `hoist_mutation_node` | `_ =>` | arm cold (486: 0) | nothing — cold for `PostIncDec` too |

**Why.** Where `PreIncDec` is an *alternative* in a multi-pattern arm, LLVM
merges it into the arm's single region, which `PostIncDec` / `Assign` /
`Literal` already cover. There is no separate counter to flip. Only the two
sites with their own arm are cold.

**And deleting the merged ones is actively worse.** Five sit behind a
catch-all, so removing the alternative produces no compile error if the
invariant ever breaks — `has_mutation_inner` and `arith_has_incdec` would
silently answer "not a mutation". The three exhaustive ones (`ir.rs:1133`,
`validate.rs:2033`, `validate.rs:3442`) are free compile-time guards: the
alternative costs nothing and its removal cannot go unnoticed.

Two are also **documented keeps** — `validate.rs:1711` argues in a comment
that past `reject_prefix_incdec`, "a top-level `PreIncDec` classifies as a
mutation, so `lower_arith_mutation_structured` turns it into an
`Assignment`", naming arith.rs:239 and arith.rs:1292 as the backstop.

### Recommendation

Take the two clean deletes — `brief_arith` (arith.rs:189) and
`emit_ir.rs:648` — by the precedent `validate.rs:2081` already sets for
`arith_brief` ("Were one to arrive anyway it falls to `_` and reads as
`...`"). `emit_ir.rs:648` is exhaustive, so it needs the DD#104 IR to simply
stop naming `:arith-pre-incdec` — a node the emitter can never receive.
Keep the rest and record why. Yield: 2 cold lines.

### Spun out

`hoist_mutation_node` (statements.rs:459-522) is a genuine **spec gap**, not
dead code: its `Assign` arm runs 10x but `CompoundAssign`, `PostIncDec`, and
`SubscriptAssign` are all zero — **28 cold lines**. Worth a spec that hoists
a mutation out of a nested arithmetic position.


## Resolved 2026-09-16 (journal 342) — DG overruled the recommendation, and was right

DG rejected the recommendation above:

> deleting a merged alternative would prevent it matching in the future,
> yes? given a disallowed usage is merged with an allowed usage - that's
> undesirable. That it cannot be measured doesn't prevent reasoning about
> it.

`x++` is allowed and `++x` is rejected; merging them means the arm is
correct only by coincidence. **Option 2 was taken** — `PreIncDec` split into
its own `unreachable!()` arm at nine sites, citing a new
`ir::PREFIX_INCDEC_INVARIANT`.

### The split found a real hole on its first run

Two of the ten spellings **ICEd**: `"$(( ++x ))"` and `"${x:-$(( ++x ))}"`.

The claim in the Evidence section above — that the two `parse_arith_str`
bypass callers "carry their own rejection" — **was false**, and it is
repeated twice in this file. They call `parse_arith_str` raw. Their SB4020
came from `validate::check_arith_value_mutation`, which caught the prefix
form *only* because `ir::arith_has_mutation` lumped it in with the
mutations. The merged alternative was load-bearing, and the symptom was
user-visible: the identical construct got the specific "prefix ++x is not
allowed" message unquoted and the vague generic mutation message inside
double quotes.

**Fix:** `reject_prefix_incdec` moved out of the two entry points and into
`parse_arith_str`, the one funnel every route passes through. The invariant
is now structural rather than asserted-where-remembered, and both
entry-point calls were deleted as redundant. `cov.253-255` pin the three
formerly-bypassing routes and assert the *message*, not just the code.

### Corrections to the measurement above

- `statements.rs:482 hoist_mutation_node` — still a spec gap, still spun
  out, unaffected.
- The "documented keeps" argument from `validate.rs:1711` is void: that
  comment described the merged behaviour and has been rewritten.
- The two "clean deletes" (`brief_arith`, `emit_ir.rs:648`) became
  `unreachable!()` arms rather than deletions, per option 2.

### Complexity fallout

Three touched functions tripped the gate; two were pre-existing
grandfathered violators. All three came down via `ArithExpr::children()`
(one place that names the shape, so each walker is a single loop) and an
`arith_assign_stmt` / `binop_on_var` / `subscript_assign_rhs` split of the
lowerer:

| function | before | after |
|---|---|---|
| `arith_has_mutation` | 14 / 2 | 4 / 1 |
| `check_positionals_arith` | 14 / 3 | 6 / 3 |
| `lower_arith_mutation_structured` | 12 / 6 | 10 / 4 |

Sweep 5229 (+6, predicted exactly), 0 failing, `mk test` 83.
