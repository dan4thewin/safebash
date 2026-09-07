# 253 — DD#74 addendum: the tail of an `&&`/`||` statement is not a predicate

**Branch:** `hand-parser`.

## The report

DG: *"sash supports `[[ ]] || cmd` but if the last clause is a user function,
it throws SB4037."*

```
fn f { echo fallback; }
x=
[[ -n $x ]] || f        # SB4037: `f` is used as a predicate but does not
                        #         `return` on every path
```

It is broader than the report's framing: **any** user function on the right of
`&&`/`||` tripped it — `false || f` too. External commands and builtins were
unaffected, which is what made it look like a `[[ ]]` interaction.

## Why it was wrong

DD#74 makes a function used in a *boolean context* a predicate, and requires
its status to come from an explicit `return`. `collect_predicate_uses_stmt`
implemented "operand of `&&`/`||`" as **both** operands:

```rust
StmtKind::List(l) if matches!(l.op, ListOp::And | ListOp::Or) => {
    mark_predicate_operand(&l.left, funcs, out);
    mark_predicate_operand(&l.right, funcs, out);   // ← the bug
}
```

But a function is a predicate when something *branches on its status*. In a
bare `A || B` statement the final clause's status is just the statement's own
status — nothing branches on it — so it is no more a predicate than a bare `f`
statement is, and a bare `f` was already accepted. The rule was inconsistent
with itself.

## The line DG drew

I checked before changing it, since DD#74 might have meant it. DG's
clarification: *"an `if a && b && c { }` should probably still require `c` to
be a predicate."* Exactly right, and it pins the distinction:

- **statement** `a && b && c` — the tail `c` is not a predicate; its status is
  the statement's status;
- **condition** `if a && b && c` — the whole list is consumed as a boolean, so
  every clause including `c` decides the branch.

Two cases stay predicates in the statement form too, and both are load-bearing:

- the **left** operand (its status decides whether the right side runs);
- a **middle** clause — `&&`/`||` are left-associative, so `a && b && c` is
  `((a && b) && c)`, and `b`'s status decides whether `c` runs.

## The fix

Delete the right-operand marking in `collect_predicate_uses_stmt`. Everything
else falls out of machinery that was already correct:

- a nested list on the **left** goes through `mark_predicate_operand`, which
  recurses into *both* its operands — so the middle clause is still marked;
- an `if`/`while` **condition** goes through `mark_condition_predicate` →
  `mark_predicate_operand`, so a condition list marks all its clauses,
  including the last.

The whole change is one deleted line plus the comment explaining why the two
paths differ.

## Verification

3-mode sweep: **0 regressions**, +24 passing (4060 → 4084; failing unchanged at
33). specs `functions.sh` 74.30–74.37, written first. Four of them are guards
that passed *before* the fix and must keep passing — the left operand (74.33),
the middle clause (74.34), and DG's `if`/`while` condition cases (74.35, 74.36).
74.37 is the strongest form of the tail case: a branching function with no
`return` on any path, which is precisely what SB4037 exists to reject in a
predicate and precisely what is fine as a tail.

One of my drafts was wrong rather than the compiler: `echo "rc=$?"` after a
suppressed statement is SB5012, so the original 74.37 failed for an unrelated
reason. Replaced rather than worked around.
