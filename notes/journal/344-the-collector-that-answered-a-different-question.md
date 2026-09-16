# 344 — The collector that answered a different question

A reachability probe went looking for two dead emitters and found four
bugs behind an `attest`.

## What the probe was for

`emit_ir::ar_incdec` and `ar_subscript_assign` were cold. The task file
`dead-arith-mutation-emitters.md` prescribed the method: temporary
instrumentation plus the **whole** spec suite, not hand-picked probes.
So `probe_hit()` went into both, keyed on `SASH_EMIT_PROBE`.

Two things had to be established before the result meant anything.

First, that the mechanism could fire at all — an empty log proves nothing
if the writer is broken. A control probe went into `ar_assign`, which the
allowed embedded assign `(( (a=2) ))` reaches. It fired.

Second, that the sweep actually exercises the emitter. It does, and more
than expected: `emit_ir` runs on **every** compile, not only under
`--emit-ir`. The control fired on a plain `sash -c … -o out`.

Result over 5259 compiled programs: only the control. Both emitters dead.

## The carve-out

Establishing *why* they were dead is where it went sideways. The
structural argument needs a rejection that holds everywhere, and there
are two gates:

- `check_arith_value_mutation` — every `ExprKind::Arithmetic`, via
  `walk_all_exprs`, rejects any mutation anywhere in the tree.
- `check_arith_test_placement` — every `StmtKind::ArithTest`, same.

Between them, no `PostIncDec` or `SubscriptAssign` survives validate. Except
`check_stmt_value_rules` has a carve-out:

```rust
if !attest_preds.contains(&(expr as *const Expr)) {
    check_arith_value_mutation(expr, diags);
}
```

DD#93 Part D: an attest's `(( TARGET = SOURCE ))` predicate is an assignment
*by design*, so SB4020 must not see it. But the exemption is for the whole
predicate expression, not its top node — and the predicate is an
`ExprKind::Arithmetic`, so `check_arith_test_placement` never sees it either.
That leaves the parser's `reject_nested_mutation` as the only gate on a
mutation nested in the RHS.

## Bug 1 — the fifth copy

`reject_nested_mutation` asked `has_mutation_inner`, a local
re-derivation of `ir::arith_has_mutation` that stopped at `BinOp` and
`Paren` and **omitted `SubscriptAssign` from the mutation set entirely**.

Journal 343 collapsed four copies of the child-walk. This was a fifth, one
layer up: not a copy of `children()` but of the predicate built on it.

So these parsed clean and reached the value path:

```
attest n (( c = n ? (d++) : 1 ))     ICE
attest n (( c = n + v[d++] ))        ICE
attest n (( c = n + -(d++) ))        ICE
attest n (( c = n + (v[0] = 3) ))    ICE
```

each landing on the `unreachable!()` in `transform::expr_handlers`. In a
plain `(( … ))` statement all four are caught by
`check_arith_test_placement`; only the attest route is exposed.

Fixed by deleting `has_mutation_inner` and calling `ir::arith_has_mutation`.
A partial re-derivation of a shared predicate is how an ICE gets built.

## Bugs 2, 3, 4 — the wrong collector

The same arm gets the reads wrong, for the same shape of reason:

```rust
crate::ir::walk_arith_var_reads(value, &mut |name| sink.collect_only(name));
```

`walk_arith_var_reads` is complete — it descends into `Paren`, `Subscript`,
everything. It just answers a **different question**. It is DD#77 D1's
"which bare int operands cross from string?", and it therefore *deliberately*
skips a subscript's subject name and a sexpr callee name, and checks nothing
for definedness. Its own doc says so.

Used as a dead-variable read-collector, that yields three defects:

| spelling | was | why |
|---|---|---|
| `attest n (( c = n + v[d] ))` | false SB5050 on `v` | subject name skipped by design |
| `attest n (( c = n + (d) ))` | false SB5050 on `d` | DD#58 nullary sexpr; callee name skipped by design |
| `attest n (( c = n + zz ))` | **accepted, `zz` read as 0** | `collect_only` never checks definedness |
| `attest n (( c = n + vv[0] ))` | ICE in codegen | same, and the undefined array reached `codegen/types.rs:190` |

The third is the serious one. Every other spelling of an undefined read —
`$(( zz ))`, `$zz`, `${zz}`, `zz[0]`, `a[zz]` — reports SB5031; validate.rs
has a comment enumerating exactly that list and calling the sexpr node "the
sole hole". It wasn't. An attest predicate was a second one, and there a
silent zero is worse than anywhere else: the construct exists to *prove*
something about a value.

Fixed by calling `check_arith_reads` — the collector a plain `(( … ))`
statement already uses. The attest target needs no special handling: that
function only descends into an `Assign`'s value, so it never sees the target.

## What the shape of this was

Both halves are the same mistake twice: a question that already had a
canonical answer in the codebase got a second, narrower answer written
next to it. 343 found four copies of one walk and called the duplication
"documented rather than fixed". Here the substitution was not even a copy —
it was a *different predicate*, close enough in name to look right at the
call site.

`walk_arith_var_reads` did nothing wrong. Asking it a dead-variable question
did.

## Ledger

- 13 specs, `cov.271-283`, covering every node kind the walk now descends
  into plus the positive controls that widening it must not break.
- Sweep **5285 passing, 10 skipped, 0 failing** (+26, predicted exactly).
  The SB5031 tightening broke nothing in the existing suite.
- Units 83 green. Touched functions 7/5 and 8/5, under the 11.9/6.0 bar.
- `ar_incdec` / `ar_subscript_assign` confirmed dead; that work follows
  separately so the bug fixes land on their own.
