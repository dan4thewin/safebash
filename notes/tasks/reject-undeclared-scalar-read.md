title: "Reject a scalar read of a name introduced nowhere in scope (typo check; DD#62 Model B)"
status: done
priority: P2
kind: feature
depends: [DD62-test-expr-simplification]
gates:
  - "echo $foo (introduced nowhere) is a compile error (SB5031)"
  - "msg=hi; echo $mgs (typo) is rejected; the message names the variable and suggests ${mgs:-DEFAULT}"
  - "x=1; echo $x compiles; echo ${x:-default} always compiles"
  - "if c; then x=1; fi; echo $x COMPILES (meaningful zero on the else path — Model B, NOT rejected)"
  - "a name introduced by assignment / int|array|map decl / for-var / read|mapfile target anywhere in the scope is 'known' (order-insensitive)"
  - "functions see enclosing (global) names; a name introduced only inside another function is unknown"
  - "specials ($?/$$/$!/$#/$LINENO) and positionals ($1/$@) are not flagged (handled elsewhere / auto-excluded); ENV_VARS/REJECTED_VARS keep their own codes"
  - "spec tests; native + c + janet identical (front-end check); no net regression"

# Reject a scalar read of a nowhere-introduced name (typo check)

> **Model B (DG, 2026-07-08 — DD#62 amendment).** NOT definite-assignment
> dataflow. Scalar zero values are meaningful (untyped→"", int→0); the only
> rejection is the **typo**: a scalar read whose name is introduced nowhere
> in scope. A known-but-maybe-unset read is allowed (yields the zero
> value). Cheap single-pass scope-membership (union/may), no branch
> intersection, no zero-trip-loop modeling, no SSA backstop. The
> superseded definite-assignment design is kept below for history.

## Model B design (implement this)

A new front-end pass `validate::check_undeclared_reads`:
1. **Collect** the "known" scalar names per scope. Top-level (globals) =
   names introduced anywhere in `program.statements` (+ end_block),
   descending control-flow bodies (if/while/for/case/list/group) but NOT
   nested function bodies. Per function: its own introductions ∪ globals.
   Introductions: assignment LHS, `int/array/map/const` declaration names,
   `for` loop var, `read`/`mapfile`/`readarray` targets.
2. **Check reads.** Walk each scope; a scalar read is `SimpleExpansion(n)`
   or a bare `ParameterExpansion{name:n, operator:None}`. If `n` is a plain
   identifier (`^[A-Za-z_][A-Za-z0-9_]*$`), not in ENV_VARS/REJECTED_VARS
   (their own codes), and not in the scope's known set → **SB5031**. A
   `${x:-…}` (operator present) is the opt-out; skip its `name` but still
   check reads inside the operand. Subscripts (`${a[i]}`) are out of scope
   (container); still recurse the index.
3. Wire into `cli_main` alongside `check_taint`/`check_positionals`.

`$LINENO` needs no special-case (parse lowers it to a `Literal`, invisible
here). CmdSubst-internal reads may be deferred (conservative — a miss, not
a false reject).

# Reject a not-definitely-assigned scalar read (SUPERSEDED — history)

## Decision (DG, 2026-06-21) — DD#62

A bare scalar read `$x` / `${x}` is a **compile error** unless `x` is
**definitely assigned on every path** reaching the read (strict
definite-assignment, not merely "declared somewhere in scope"). The
explicit opt-out is the default form **`${x:-WORD}`** (DD#65), always
allowed, which is the only way a scalar **zero value** is now reached.

```
echo $foo                      # reject — assigned nowhere (typo class)
msg=hi; echo $mgs              # reject — typo
if c; then x=1; fi; echo $x   # reject — c-false path reaches $x unassigned
x=1; echo $x                  # ok
echo ${x:-default}            # ok (explicit maybe-unset)
```

## Why

The headline safe-subset value: a never-assigned / typo'd variable read
is the single biggest shell footgun (`rm -rf "$PREFIX/"` with a typo'd
`$PREFIX`). sash is a compiler with full visibility, so it can reject this
at compile time. `${x:-…}` already expresses "maybe-unset," so erroring on
bare reads costs no expressiveness — it forces intent.

## Where / feasibility

- `type_check.rs::TypeEnv` already tracks declared variables/types — the
  starting point for "is this name known."
- The **all-paths** ("definitely assigned") analysis wants a CFG dataflow
  join. sash already builds SSA with **phi insertion at branch joins**
  (`transform/`), so the join machinery exists — the check can run on the
  SSA/CFG (a name unassigned on some predecessor → its phi/use is
  "maybe-unset") or as a dedicated definite-assignment pass over the IR
  CFG. Decide placement (pre-SSA IR pass vs piggyback on SSA) in design.
- `${x:-…}` reads bypass the check (explicit opt-out).

## Sub-questions to settle in design

- **Typed zero-init**: `int x` (no initializer) → is `x` assigned to `0`?
  `array a` → empty? Proposed: a typed declaration zero-inits and counts as
  assigned. Confirm the exact declaration set that zero-inits.
- **Loop variables**: after `for i in LIST { }` or a `while`, is `$i`/a
  body target "assigned"? A possibly-empty loop does not assign → reading
  after the loop is rejected unless pre-assigned. (Inside the body, the
  loop var is assigned.)
- **Positionals** (`$1`/`$@`/`$#`): governed by DD#46 (function-local,
  top-level SB error) — orthogonal; not scalars this check owns.
- **SB code**: assign next-free at implementation.

## Discovery

DD#62 decision #3 open item, resolved 2026-06-21 in the DD walkthrough.
Current behavior: undeclared/typo'd scalar reads compile and yield empty
silently (verified).

## Resolved (DG, 2026-06-21) — narrowed design

- **Scalars only.** Definite-assignment rejects a bare scalar read `$x`/
  `${x}` that is not definitely assigned on every path. `${x:-WORD}` is
  the opt-out.
- **Two passes (mirrors taint):** an **IR-pass** is primary — it carries
  source spans, so the diagnostic points at the read and suggests
  `${x:-}`; the **SSA** check is a soundness backstop that **ICEs** if it
  catches a maybe-unset read the IR-pass missed (version-0 / phi-source
  propagation already present in SSA). Exclude compiler-managed specials
  (`$1`/`$@`/`$?`/`$$`/`$LINENO`…) and the `${x:-}`/`${x:?}` forms.
- **Typed scalar decl = assigned.** `int x` (no initializer) zero-inits to
  `0`, so `$x` after it is fine (matches today).
- **Containers are OUT of scope.** `array a`/`map m` are always assigned
  (a valid *empty* container); definite-assignment never applies. An empty
  container's only consequence is cardinality at an **external `@a`
  splat**, handled by the disposition (`@{a:-}`/`@{a:?}`) — NOT here.
- **Loop targets:** a possibly-empty `for`/`while` does not assign its
  body's scalar targets (reading them after the loop is rejected unless
  pre-assigned); the loop variable is assigned inside the body.
