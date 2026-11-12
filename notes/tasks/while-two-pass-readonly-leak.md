title: "`lower_while`'s two-pass lowering leaks `readonly_vars` into pass 2"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "either `const r=…` inside a `while` body behaves the same as inside a `for` body, or a DD says why a loop body may not bind a const at all"
  - "whichever way it goes, a spec pins it"

## Found

2026-09-11, while fixing the `read ARR`-in-a-`while`-body segfault
(journal 265). Same root cause, different symptom, and deliberately **not**
fixed with it.

## What

`lower_while` (`transform/control_flow.rs`) lowers the body twice: pass 1
discovers loop-carried variables, then `insts.clear()` discards its
instructions and pass 2 re-lowers. Pass 1's mutations to `state` are not
rolled back.

The segfault fix restores `collections` and `assoc_arrays`, because those gate
*emission* — skipping the empty-collection `Assign` left a bulk-read target
with no table and the fill wrote into the `""` initializer.

`readonly_vars` leaks the same way, but it gates a *diagnostic*:

```sh
int n=0
while (( n < 2 )) {
  (( n = n + 1 ))
  const r="x"
  echo "$r"
}
```

→ `SB4044 readonly variable 'r' cannot be reassigned`, from the SSA backstop
(`ssa.rs`, the `1:1:` spelling), because pass 1 already registered `r`.

## Why it was not fixed with the rest

Restoring `readonly_vars` would make `while` **accept** this. But `for` rejects
it too:

```sh
for i in 1 2 { const r="x$i"; echo "$r" }   # also SB4044
```

and there the rejection is legitimate — `for i in 1 2` unrolls
(`lower_for_static`), so the second copy really is a second binding of `r`.

So the two loops agree today, for different reasons: `for` is right on the
merits, `while` is right by accident. Restoring `readonly_vars` would split
them, and there is no ruling saying which way they should go.

## The actual question

Should a loop body be allowed to bind a `const`? Every iteration re-binds it,
which is either (a) fine, because the binding is scoped to the iteration and
immutable *within* it, or (b) not, because `const` means one binding for the
life of the program (the flat model `validate.rs` uses for SB4044).

Both are defensible and it is a language-surface question, so it is DG's.

Current behaviour — reject in both loops — is the safe default: no wrong
answer, no crash, and it is what ships today. This is filed to record that the
`while` half rests on a bug, so whoever changes the two-pass lowering knows
that fixing the leak has a visible consequence here.

## Note

The version counters must **not** be rolled back — `VersionState::counter` is
documented as a high-water mark, and `insert_phis_two_branch` consumes pass
1's versions. Only guard-then-skip state is in question.

## Resolved 2026-09-11 — DG ruled; the leak is now irrelevant (SB5072)

> "const shouldn't be within loop"

This filing asked whether a loop body may bind a `const`, because restoring
`readonly_vars` across `lower_while`'s two passes would have made `while`
accept it while `for` still rejected it. DG ruled: **no** — it follows from
DD#86's no-block-scope model, since a loop body introduces no scope and the
binding is simply re-executed.

So the leak is not fixed, and no longer needs to be: the construct it was
accidentally rejecting is now rejected **deliberately**, by `check_const_in_loop`
in `validate.rs`, with a real span. Previously it surfaced as the spanless
`VersionState::define` backstop reported at `1:1`.

`const` inside a function body stays legal — DD#86 Phase 2 fixed the identical
leak there via `enter/exit_fn_body_scope`, and spec 86.30 keeps it fixed.

The one note worth carrying forward: version counters must still **not** be
rolled back across the two passes (`counter` is a documented high-water mark
that `insert_phis_two_branch` consumes). Only guard-then-skip state may be.
