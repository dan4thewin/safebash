# 308 — The validator was saying everything twice

Branch coverage was supposed to be a tidy numbers exercise. DG picked
"pursue branch misses" off the queue, the run finished at **84.21%, 599
missed**, and the first thing the ranking showed was not a function. It was
a *shape*: `if let Some(ref end) = program.end_block` appearing at twelve
separate uncovered sites in `validate.rs` and two more in `resolve.rs`.
Twenty branches — 3.4% of every miss in the compiler — in one construct.

The obvious read was that END blocks are untested. They are not: `spec/
signals.sh` has thirty-odd DD#56 tests and `spec/exit_arg.sh` 89.8 checks a
full i64 reaching END through `$?`. All passing, in both modes.

So the arms got read directly:

```
sash/src/resolve.rs|60|0|0|0
sash/src/resolve.rs|60|0|1|3632
```

Zero out of 3632. Meanwhile `codegen/entry.rs:397` — also an `end_block`
test — showed 11 hits. Codegen sees END blocks. Validate never does.

## `Program::end_block` is dead

`--emit-ir` settles it in one line:

```
{:ir-version 1
 :statements [... {:kind :function :line 2 :name "__sash_end__" ...}]
 :end-block nil}
```

The parser lowers `END { }` into `statements` as a function named
`__sash_end__` ("for backward compatibility with transform",
`parser.rs:790`). The field it *could* have used is initialised at
`parser.rs:38`:

```rust
let end_block: Option<Vec<Statement>> = None;
```

Not `mut`. Never reassigned. `ir::Program::end_block` is `None` for every
program sash has ever compiled. The fourteen `if let Some(ref end)` blocks
guarding it are dead, and `codegen/entry.rs:397` is a different field on a
different type (`SsaProgram::end_block`, an `Option<BlockId>`) that the
transform *does* populate.

Nothing is broken by this. END bodies are validated because they are
ordinary functions in `statements`, and `validate.rs:1992` special-cases
`__sash_end__` by name. The field is vestigial, not load-bearing. What it
cost was twenty branches of permanent red in the two passes that matter most
for a compiler that is supposed to be a safety gate.

## Which is how the real bug surfaced

Checking that END really was validated meant compiling `END { echo $HOME }`
and looking for SB5018. It was there. Twice.

```
$ sash -o t t.sh
t.sh:3:6: error [SB5018] $HOME is not available directly — use $(getenv HOME)
t.sh:3:6: error [SB5018] $HOME is not available directly — use $(getenv HOME)
```

Not an END artifact. Every nesting construct did it:

| program | SB5018 count |
|---|---|
| `echo $HOME` | 1 |
| `fn f { echo $HOME }` | 2 |
| `if true; then echo $HOME; fi` | 2 |
| `while false; do echo $HOME; done` | 2 |
| `{ echo $HOME; }` | 2 |
| `echo $HOME \| cat` | 2 |
| `true && echo $HOME` | 2 |
| `fn f { if true; then echo $HOME; fi }` | **3** |

Depth + 1. Not specific to SB5018 either — `$IFS` gave two SB5017s, and any
diagnostic raised from `validate_expr` behaves the same way.

## Cause

`validate_stmt_ctx` does two things:

```rust
match &stmt.kind {
    StmtKind::If(i) => validate_if(i, diags),
    ...
    _ => validate_nested(stmt, in_condition, diags),   // recurses into child_bodies()
}
walk_exprs_in_stmt(stmt, &mut |expr, span| validate_expr(expr, span, diags));
```

The dispatch recurses into every nested statement, and each of those
recursive calls runs the same trailing walk on itself. But the trailing walk
is `ir::walk_all_exprs`, which is *deep* — it ends with

```rust
for body in stmt.child_bodies() {
    for s in body { walk_all_exprs(s, f); }
}
```

So an expression N levels down is visited once by its own statement's walk,
once by its parent's, once by its grandparent's. Two full tree recursions
multiplied together.

## Fix

`Statement::direct_exprs` already existed and already said exactly what was
needed — *"Visit all expressions directly owned by this statement
(non-recursive). Does NOT descend into child statements — use
`child_bodies()` for that."* `walk_all_exprs` was the only thing standing
between it and correctness.

Split the recursion out. `ir::walk_own_exprs` is now the shallow half
(`direct_exprs` expanded through `walk_expr_tree`, plus the
`walk_arith_expansions` arm for a `(( ))` statement's `ArithExpr` tree);
`walk_all_exprs` is that plus the `child_bodies` loop, unchanged for its
three deep-scan callers (`find_dollar_question`, `stmt_uses_status`,
`check_regex_replacements`). `validate_stmt_ctx` uses the shallow one.

## Proving nothing was lost

The risk in this shape of fix is not duplicates — it is *silent acceptance*:
if `child_bodies()` failed to reach some sub-statement the deep walk did, a
shallow walk would drop a rejection and turn a compile error into a compiled
binary. In a validator that is the worst possible regression.

So it was measured rather than argued. A forty-program battery covering
every nesting construct — function bodies, `if`/`elif`/`else`, `while`, `for`
word lists, dispatch subjects and patterns, groups, pipelines, `&&`/`||`,
suppression, `END`, redirect targets, nested quotes, command substitution,
declarations, array literals, arithmetic — was run against the compiler
before and after, capturing every diagnostic line.

**Before: 90 lines. After: 51. Set difference: empty** (modulo the PID in a
temp path). Every diagnostic that was reachable is still reachable; only the
repeats are gone.

Then the reverse check, which matters just as much: revert the one-line
call, rebuild, re-run. `0.8` and `0.9` fail. `0.7`, `0.10` and `0.11` — the
"nothing lost" guards — pass in both directions, which is what a guard
should do.

## Specs

`errors.sh` 0.7–0.11, in the `0.x` block the file reserves for walker bugs
(0.5 and 0.6 are the 2026 elif-skipping pair — same family: a walk that
visits the wrong set of statements produces either spurious diagnostics or
silent acceptance). Plan 181 → 186. Sweep **4578 passing, 9 skipped, 0
failing**.

0.11 was written against `case … esac` first, which was a nice reminder that
DD#66 removed it; it is now the `[[ $x == pat { … } ]]` dispatch form, which
exercises the same thing — a statement with two direct expressions that must
each be validated exactly once.

## What this says about the campaign

The coverage number was never the point, and this is the clearest evidence
so far. 84.21% branches is a fine number. The 20 branches behind
`program.end_block` were not a testing gap — they were a dead field nobody
had noticed in the two passes where dead code is least acceptable. And
following *why* they were red led to a bug that has presumably been printing
doubled errors at users for as long as the walk has had this shape, in a
compiler whose entire value proposition is the quality of its rejections.

Coverage did not find the bug. Coverage found the *lie*, and the lie was
adjacent to the bug.

## Open

`ir::Program::end_block` is still there. Removing it takes 20 branches off
the miss list, but `:end-block` is a key in the DD#104 IR capture, which is a
versioned artifact format — so that is a question for DG rather than a
cleanup. Filed as `notes/tasks/dead-ir-end-block-field.md`.
