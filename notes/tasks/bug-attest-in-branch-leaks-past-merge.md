---
title: "Taint bypass: an `attest` inside conditional control flow cleans the name for everything after it"
status: open
priority: P1
kind: bug
depends: []
gates: []
---

# The taint set is mutated linearly, with no join at control-flow merges

Found 2026-09-30, answering DG's question "what if the order of calls depends
on a runtime branch". The answer is that the top-level taint walk does not
model branches at all.

## Reproducers — all three compile, all three escape

**(1) attest in `then`, read in `else`.** The attest provably did not run on
the path that reads.

```sash
str c=$(getenv COND)
attest c [[ -n $c ]]
argv a
if [[ $c == yes ]]; then
  attest v in a [[ -n $v ]]
else
  for v in @a; do /bin/echo "$v"; done
fi
```

`COND=no ./q SECRET1 SECRET2` prints `SECRET1`, `SECRET2`.

**(2) attest in `then`, read after the `if`.**

```sash
if [[ $c == yes ]]; then attest v in a [[ -n $v ]]; fi
for v in @a; do /bin/echo "$v"; done
```

`COND=no` prints the argv.

**(3) a *caught* gate that fails, read after the `if`.**

```sash
argv a
if attest v in a [[ $v == IMPOSSIBLE ]]; then /bin/echo p; fi
for v in @a; do /bin/echo "$v"; done
```

The gate cannot pass, so the retag-clean never happens — but the read compiles.

The same read with no attest anywhere is correctly `SB5020`, and an attest on a
*different* array is correctly `SB5020`. It is specifically a correctly-named
attest **anywhere textually earlier**, regardless of reachability, that
suppresses the diagnostic.

## Mechanism

`taint_top_level` walks statements in order, mutating one `HashSet<String>`:

```rust
for stmt in program_blocks(program).into_iter().flatten() {
    if let StmtKind::Function(f) = &stmt.kind { deferred.push(f); }
    else { taint_stmt(stmt, tainted, cx, diags); }
}
```

`program_blocks(...).flatten()` yields the statements of every block —
including branch bodies — as one flat sequence. An `attest` inside an `if`
therefore *removes* the name from the single shared set, and every statement
walked afterwards (the sibling `else` block included, since `then` is flattened
first) sees it clean.

Flow-sensitivity of the ordinary kind does work — `argv a; attest; read` is
accepted and `attest; argv a; read` is `SB5020` — so the walk is sequence-aware.
It is *branch*-unaware: there is no join, so a removal on one path is treated
as a removal on all paths.

## What the intended model is

DD#24's amendment (2026-09-01) specifies branch-scoped cleanliness for the
caught form: `lower_if_bless` is "branch-on-test, **retag-clean-on-pass**", and
`bless … || X` lowers to `if bless {} else { X }`. So the language already
says a caught gate cleans the subject **on the passing branch only**. The
validator does not implement that; it cleans unconditionally and permanently.

## Proposed fix

A removal must be scoped to the block it occurs in, and a merge must take the
**union** of the tainted sets of the incoming paths (tainted on either path =
tainted after). Concretely: `taint_stmt` should walk a branching statement by
cloning the set per branch, walking each, and unioning the results back —
the opposite of the discard used for function bodies, because these blocks are
*paths through the same scope*, not nested scopes.

Additions can stay as they are: a taint added on one path must also be tainted
after the merge, and union preserves that.

Note this is the same underlying shape as
`bug-slot-inward-taint-reads-final-set` — both are the taint set being consulted
or mutated at the wrong program point — but they are independent defects with
independent fixes.

## Related crash

The read this bug wrongly admits is *also* a memory-safety fault in the caught
form: see `bug-caught-attest-all-fail-path-undefined-handle`. Fixing this bug
makes that crash unreachable from safe source, but the codegen defect should be
fixed on its own terms.

## Why it went unseen

`spec/taint.sh` covers the caught-gate forms (`bfb.1`-`bfb.6`) but always reads
the subject *inside* the guarded branch, which is the correct shape. No spec
places the read after the `if`, or in the sibling branch.

## Found by

DG: "this seems to use the order of calls while the other avoided using the
order. what if the order of calls depends on a runtime branch".
