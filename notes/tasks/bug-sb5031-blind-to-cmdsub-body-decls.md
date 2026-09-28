---
title: "SB5031 reports a `$( )`-body array declaration as 'introduced nowhere'"
status: next
priority: P2
kind: bug
depends: []
gates:
  - "`/bin/echo $(array a=( ); array b=( @{a:-z} ); echo $b[0])` compiles and prints `z`"
  - "`@nosuch` is still SB5031 everywhere it is genuinely undeclared"
  - "the false-rejection/false-acceptance tradeoff below is DG's call, not the implementer's"
---

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

## Why this was not just fixed

Widening `program_scopes` to include `$( )` bodies trades a false **rejection**
for a false **acceptance**, and which one sash wants is a scoping decision:

- DD#86's addendum says a `$( )` body is an anonymous function, so names first
  introduced inside are **local**. Unioning them program-wide would make `@a`
  legal *outside* the body that declared it.
- `program_scopes` already does exactly that for function bodies, and says so:
  *"a program-wide union covers names introduced inside functions too. Sound
  because sash forbids shadowing: one name, one meaning, program-wide."* So the
  imprecision would not be a new class, only a wider instance of an accepted
  one.

Either answer is defensible. It is DG's call.

## Implementation note, once the semantics are settled

`program_scopes` needs a **lifetime-preserving** shallow cmdsub-body accessor,
and there isn't one. `Statement::child_bodies(&self) -> Vec<&[Statement]>`
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
