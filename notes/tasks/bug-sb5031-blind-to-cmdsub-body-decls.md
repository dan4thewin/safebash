title: "SB5031 reports a `$( )`-body array declaration as 'introduced nowhere'"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`/bin/echo $(array a=( ); array b=( @{a:-z} ); echo $b[0])` compiles and prints `z`"
  - "`@nosuch` is still SB5031 everywhere it is genuinely undeclared"
  - "`fn f { array a=(1 2); … }; f; /bin/echo @{a:-z}` is SB5031 — a fn-local name is not readable outside"
  - "`check_scalar_splat` reads the scope-aware `known`, not a flat program-wide union"

> **Fixed 2026-09-28 (journal 437).** The splat read is recorded through
> `ReadSink::record_splat` against the current scope's `known`, like every
> other read; `check_scalar_splat` keeps only SB5079 and its flat
> `program_scopes` union no longer decides existence. Both directions verified:
> the in-body splat compiles, the function-local splat at top level is SB5031.
> Specs 75.108-75.111. Sweep 6732 passing / 13 skipped / zero failing (6745,
> predicted). No `direct_exprs` lifetime change was needed — the scope-aware
> traversal already descends into `$( )` bodies, which is why the scalar and
> subscript spellings were right all along.


# The reporting side descends into a `$( )` body; the introduction side does not

A valid program is rejected:

```sh
/bin/echo $(array a=( ); array b=( @{a:-z} ); echo $b[0])
```

```
error [SB5031] `@a` splats `a`, which is introduced nowhere — declare it with
`array a` / `map a`, or correct the spelling
```

`a` is declared **one statement earlier, in the same body**. Found while
probing the splat-disposition taint bug (journal 436); it is what forced those
specs to declare the array outside the body.

## Diagnosis

`check_scalar_splat` (`validate.rs:6733`) builds its `introduced` set from
`collect_scope_names` over `program_scopes(program)`, then reports with
`walk_stmt_tree` + `walk_expr_tree`.

The two halves disagree about what a `$( )` body is:

| half | reaches a `$( )` body? |
|---|---|
| reporting (`walk_expr_tree`) | **yes** — it sees the `@a` inside |
| introduction (`program_scopes`) | **no** — `child_bodies()` + `function_body` only |

`program_scopes` enumerates top-level blocks, control-flow bodies, and
function bodies. A `$( )` body is a statement list reached through an
**expression**, so `Statement::child_bodies()` — which is total over
`StmtKind` — structurally cannot yield it.

This is the same asymmetry as the SSA taint bug: one walk total, its partner
not, and the gap is where the wrong answer lives.

## There is no semantic decision here — DD#86 already settles it

The first filing of this bug said the fix traded a false **rejection** for a
false **acceptance**, and called that DG's call. That was wrong, and it was
wrong because only one direction had been measured.

`check_scalar_splat`'s flat program-wide union is not a conservative choice.
It is wrong in **both** directions, and the other direction is the worse one:

```sh
fn f { array a=(1 2); /bin/echo @{a:-z}; }
f
/bin/echo @{a:-z}          # ACCEPTS, and prints `1 2`
```

DD#86: *"a name first assigned inside a function that is not a top-level global
is automatically local."* So that top-level read must be SB5031. The scalar and
subscript spellings of the same read both reject, correctly:

| read of a fn-local name, at top level | result |
|---|---|
| `/bin/echo $y` | SB5031 ✓ |
| `/bin/echo $a[0]` | SB5031 ✓ |
| `/bin/echo @{a:-z}` | **accepts, and runs** ✗ |

So the splat check leaks a function-local array to top level at runtime, and
separately rejects a body-local one inside its own body. One root cause, both
signs.

## The correct model is already implemented next door

DD#86 draws the line by scope, not by program:

- a body-introduced name read **inside** that body → legal
- the same name read **outside** it → SB5031 (the DD's own worked example,
  `str s=$(int y=5; echo hi); echo $y`)

Both halves already behave correctly for scalars and subscripts, because the
main SB5031 check is **scope-aware**: `check_reads_in_scope` threads a `known`
set per scope, and `SinkMode::Flag` is documented as *"flag a read whose name
is not in the **current scope's** `known`"* (`validate.rs:7903`). That file
also states the design intent — *"the one traversal below drives a single
`record` per variable read; the sink decides what to do — so the two passes can
never diverge on 'what counts as a read'."*

`check_scalar_splat` was bolted on later (its own comment explains why: a splat
is not a scalar read, so `@nosuch` was reaching codegen and ICE'ing) and it did
**not** join that traversal. It built its own flat union instead. It is the one
read-check that diverged, which is exactly what the shared-sink design was
written to prevent.

## The fix

Fold the splat read into `check_reads_in_scope` / `ReadSink` so it is recorded
against the current scope's `known`, like every other read. `splattable` is
still needed to keep SB5079 (`@x` on a scalar) distinct from SB5031 (`@x` on
nothing), so the sink needs that set threaded alongside.

Then delete `check_scalar_splat`'s `program_scopes` union. Nothing else should
need widening: the scope-aware traversal already descends into `$( )` bodies,
which is why the scalar and subscript cases were right all along.

## Implementation note

The `program_scopes`-widening approach below was the **wrong** fix and is kept
only to record why. If it is ever revisited, note that `program_scopes` would
need a **lifetime-preserving** shallow cmdsub-body accessor, and there isn't
one. `Statement::child_bodies(&self) -> Vec<&[Statement]>`
preserves the lifetime; `direct_exprs(&self, f: &mut impl FnMut(&Expr))` and
`walk_expr_tree` do not — their callbacks take an anonymous lifetime, so a
borrowed body cannot escape the closure (the same wall hit in journal 435).

The smallest unlock is one annotation:

```rust
pub fn direct_exprs<'a>(&'a self, f: &mut impl FnMut(&'a Expr))
```

Existing callers pass closures over `&Expr` and are unaffected. With that, a
shallow `cmdsub_bodies(&self) -> Vec<&[Statement]>` is a dozen lines.

The walk must be **shallow** (the existing `for_each_cmdsub_shallow` pattern,
`validate.rs:3087`), letting the outer worklist recurse — a deep walk reports a
nested `$( $( … ) )` twice, and `program_scopes`' result is iterated twice, so
duplicates become duplicate diagnostics.

## Wider finding

45 walks in `validate.rs` recurse via `child_bodies()`; a separate,
hand-written subset descends into `ExprKind::CommandSubstitution`. Nothing
makes a walk choose deliberately. Every one that reports inside a body but
collects outside it has this bug latently. Two are already known and filed
(`bug-taint-escape-in-cmdsub-body`, and SB5050 not reaching a body).
