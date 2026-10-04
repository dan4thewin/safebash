title: 473 — the pin that could not pin, and the gate that could not see

DG, on my classification of the 42 non-ICE unit tests:

> "crashing is strictly worse than being conservative" is essentially "an
> uncaught regression is ok". why do exhaustiveness pins not ICE?

Two sentences, two of my categories gone.

## The first one is my own argument, used against me

I had marked `cmd_subst_head_resolution_fails_closed` as the one place where
"spec gap or ICE, no third thing" might need a third thing: both shapes it
tests are validate-shadowed (SB4045, SB5011), but it resolves them to
`Tainted`, and I argued that being conservative beats crashing.

That is defense-in-depth again — the exact argument DG had ruled on two hours
earlier, wearing different clothes. If the shadow lifts and the shape arrives,
`Tainted` is not a safe outcome, it is a **silent** one: the program compiles,
nobody learns the gate moved, and the only record that anything changed is a
taint verdict nobody asked for. "Conservative" describes the value returned.
"Uncaught" describes everything else about it.

I had even written the counter-argument into AGENTS.md myself, in the taint
walker's voice: *silent-and-maybe-wrong is the worst option*.

## The second one dismantles a category I invented

I had kept four tests as "exhaustiveness pins" — `to_tok_maps_every_reachable_
variant`, `atok_to_text_spells_every_token`, `every_expr_kind_renders`,
`logos_matches_every_dispatched_structural_byte` — on the grounds that a spec
cannot assert "every variant".

Read them. All four are **hand-written lists**. `to_tok_maps_every_reachable_
variant` enumerates 17 pairs. Add an eighteenth `LogosTok` tomorrow and rustc
stops the build at `to_tok` — which is total — you add the arm, and the test
passes unchanged, still at 17, still called *every*.

So the pin does not pin. What actually holds exhaustiveness is two things, and
neither of them is a test:

1. **rustc**, because the match has no wildcard;
2. **`contrib/totality-check.py`**, because it is what stops someone putting a
   wildcard back.

Everything else the test asserts — that `AndAnd` maps to `AndAnd` — is
observable from a spec, since every one of those tokens appears in sash source.
The category was a third statement of a property already stated twice, and the
weakest of the three.

## Which is how I came to look at the gate

If the gate is the real enforcement, the gate had better work.

`totality-check.py` opted a function in with `if MARKER not in line`. A
per-line search. rustfmt wraps a doc comment wherever the column limit falls,
and in `command_args_elidable` it fell inside the marker:

```
/// **This is an allowlist, and it is total on purpose — do not add a
/// wildcard.**
```

Ungated since the day it was written. That function is the DD#84 elidable-
position allowlist, total over `BuiltinCmd` on purpose, and the convention
that was supposed to hold it had never been applied to it. 15 functions opt
in; 14 were visible; that was the one.

The detail that makes it worse: the marker sentence this very file recommends
is longer than one line by construction. The gate's own documentation invites
the failure.

Fixed by joining the doc-comment run before searching. And the gate now
**self-tests on every invocation** — two crafted functions, one marker wrapped
and one not, and if it fails to flag either it exits 2 rather than reporting a
clean tree. It could not fail for the wrapped case for its whole life and
reported clean the entire time. Verified both ways: rc=0 on the wrapped case
before the fix, rc=1 after; and with `WILDCARD` sabotaged the self-test reports
2 failures.

## What else the sweep turned up

19 functions claim totality in prose without opting into the gate. Three looked
like outright lies and none were: `composite_to_value` says "total *only over*
the variants routed here" and its wildcard is an `unreachable!`, and
`CheckedArithOp::from_arith` is an `Option` mapping where `_ => None` is the
documented design.

`from_arith` is worth a second look even so. It answers "is this op overflow-
checked", and a new `ArithOp` added tomorrow gets `None` — silently unchecked —
rather than a compile error. That is the shape the convention exists to catch,
sitting inside a function whose doc comment says "total". Raised with DG rather
than changed.

The remaining 16 are clean and should simply get the marker, which is the
cheapest conversion of prose into enforcement available.

## Where the count went

The keep column of the unit-test classification was 10. After this it is six:
the three `payload_*` tests (they render a *caught* ICE, so no spec can reach
them), and the cross-API invariants a spec cannot see. The four pins join the
20 rivals, and `cmd_subst_head_resolution_fails_closed` joins the ICE
conversions.
