---
title: The index that was never an index
date: 2026-10-04
---

A coverage-queue entry, picked only because it sat in code I had just
touched, turned into a silent miscompile.

`parse_split_subscript_index` had five cold lines — the SB4014 arm that
fires when the LHS subscript has no closing `]`. I set out to prove it
unreachable. It is: the shape probe's lookahead window `[1, 32]` maps exactly
onto the consumer's `[0, 31]` once the `name[` word is consumed, so a probe
that said yes guarantees the consumer finds something.

But proving *that* meant reading both searches side by side, and they were
not looking for the same thing.

```rust
// the probe
matches!(&scanner.peek_n(i).token, Tok::Word(rb) if rb == "]")
    && matches!(scanner.peek_n(i + 1).token, Tok::Eq | Tok::PlusAssign)

// the consumer
.find(|&i| matches!(&scanner.peek_n(i).token, Tok::Word(w) if w == "]"))
```

The probe wants a `]` **followed by an assignment operator**. The consumer
takes the **first** `]`. Two searches, two predicates, one bracket. They
agree whenever there is only one `]` in the index — which is every int-array
subscript, because a second `]` there is not expressible.

On a map it is. A string key makes the extra text legal to the lexer:

```sh
map m=( )
s="k"
m[$s ] x ]="z"
```

That compiled. Clean. Exit 0. `--emit-ir` says it assigned the literal
string `]=9` to the key `zz` — the ` x ` is simply **gone**, discarded
without a diagnostic.

## How source goes missing

The consumer closed on the first `]`, leaving the scanner parked on ` x `.
Then, in `parse_assignment`:

```rust
} else {
    scanner.next(); // consume =
    AssignOp::Set
};
```

It does not *consume* the `=`. It consumes whatever is there and calls it
the `=`. The comment states an invariant the code never checks, and the
probe's promise — "there is a `]` with an `=` after it" — was about a
different `]` than the one the parser had just used.

Two defects, neither harmful alone, and each one's correctness argument
quietly depended on the other being right.

Both are closed. `lhs_index_close_from(scanner, base)` is now the single
search with the single predicate; the callers differ only by whether `name[`
is still unconsumed, which is the `base`. And the operator is consumed by
`parse_assign_op`, an explicit `match` that returns SB4014 on anything else.
(It is a separate function because inlining the `match` pushed
`parse_assignment` to cc 12 against a 3× bar of 11.34 and the gate rejected
the commit — the right answer, as it happens: "which operator is this" is a
decision worth a name.) That arm is
unreachable too — but it is now unreachable *structurally*, because one
function decides which `]` closes the index, rather than by the coincidence
of two hand-matched lookahead windows. I left it checked rather than
asserted for the obvious reason: the previous version asserted, and was
wrong.

## The differential, and the second bug

I built a pristine worktree at HEAD to separate what I had changed from what
I had merely found. (It needs the runtime copied in — `SASH_RT_LIB` did not
work; `cp libsash_rt.a <worktree>/sash-rt/target/release/` did.)

The discard was mine to fix and the fix held. But two shapes were
**identical in HEAD and in my tree**:

```sh
m[ a b ]=9      # key `a b`
m[ $s x ]=9     # key literal `$s x` — $s never expanded
```

Both accepted. Both wrong. And both already rejected — SB4041 — when
*read*. Measured across the matrix, the write side was enforcing DD#80's
index rule **not at all**: `m[a-b]=`, `m[a.b]=`, `m[ ] ]=` and
`m[ ] x ]=` all compiled too, while `${m[a-b]}` and friends rejected.

The cause is a shape. An assignment's LHS index lives in
`Assignment.index: Option<Expr>` as a bare `Expr`, never wrapped in an
`ExprKind::Subscript`:

```rust
let index = match &e.kind {
    ExprKind::Subscript { index, .. } => Some(index.as_ref()),
    ExprKind::ParameterExpansion(pe) => pe.index.as_deref(),
    _ => None,
};
```

`assignment_exprs` *does* hand `a.index` to the walker. The walker received
it, looked for a wrapper that was not there, and saw an ordinary `Literal`.
The check was reaching the index and could not tell it was one.

So the fix is a second visitor, `flag_assignment_index`, over
`walk_stmt_tree`, calling the same `ambiguous_index_diag`. One rule, two IR
shapes, two visitors. Trying to do it in `flag_subscript_index` cannot work:
an `Expr` alone does not know what position it occupies.

## Third time

This is the **third** time DD#80's enforcement has turned out to be
position-dependent rather than rule-dependent — after the write surface in
September, and the assignment/declaration *value* that was never walked at
all (journal 462). It is the same shape as the stored lesson about
`validate.rs` guards keyed on syntax rather than on the thing guarded.

The question to ask when adding a rule here is not "does the check fire on
`${a[K]}`". It is "what are all the IR shapes this thing can inhabit, and
does the check see every one of them".

Five forms flipped from accept to SB4041; the nine legal write spellings
(integer literal, `$var`, `$(( … ))`, bareword key, quoted key, `${var}`,
`+=`) were measured before and after and are untouched. SB4041 also lost the
word "braced" and its `${ }` reference — it is one rule for all positions
now, so it should not name one.

## Gates

`mk test` green. Sweep green at the predicted total both times: 7473 before
the new specs (proving nothing already pinned the old acceptance), 7507
after — native 3856, c 3638, 13 skipped, 17 new ids in a dual-mode file.
Sections P (`80.97`–`80.106`) and Q (`80.107`–`80.113`) of
`spec/subscript.sh`.

The cold lines that started this are still cold, and still should be.
