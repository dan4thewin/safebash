---
title: Dead ir::Program::end_block field
status: done
depends: []
gates: []
---

# `ir::Program::end_block` is always `None`

> **DONE** (journal 309). DG chose option 3 — wire it up. `parse()` now
> assigns the field; SB4054 (nested END) and SB4055 (second END) enforce
> DD#56's "exactly one, top level". The lost rejection this exposed
> (`check_dollar_at` blind to END) is fixed. Specs `signals.sh` 56.36-56.38,
> `misc.sh` 104.5.

## Evidence

`sash/src/parse/parser.rs:38`:

```rust
let end_block: Option<Vec<Statement>> = None;
```

Not `mut`, never reassigned, moved into the `Program` at line 74. The
parser lowers `END { }` into `program.statements` as a function named
`__sash_end__` instead (`parser.rs:790`, comment: "for backward
compatibility with transform").

Confirmed by `--emit-ir` on a program with an END block: the END body
appears as `{:kind :function :name "__sash_end__"}` inside `:statements`,
and `:end-block` is `nil`.

Confirmed again by branch coverage: `resolve.rs:60` shows arm 0 taken **0**
times against arm 1 taken 3632 times, across a sweep that compiles 30+ END
programs.

`codegen/entry.rs:397` and `inst_handlers.rs:692` are **not** this field —
they read `SsaProgram::end_block`, an `Option<BlockId>` that
`transform/entry.rs:102` does populate. That one is live.

## What is dead

14 guarded blocks, ~20 uncovered branches:

- `sash/src/validate.rs` — 294, 298, 604, 612, 2925, 3082, 3416, 4944,
  4951, 4956, 4964, 5405, 5409, 5531
- `sash/src/resolve.rs` — 60, 66
- the field itself (`ir.rs:466`) and its initialiser (`parser.rs:38`, 74)

## Why it is not a bug

END bodies *are* validated and *are* resolved — as ordinary functions in
`statements`. `validate.rs` special-cases `__sash_end__` by name for scope
purposes, and `transform/statements.rs:159` turns the function straight back
into `state.exit_trap`. Behaviour is correct.

## Correction: not vestigial — unfinished

This file first called the field vestigial. That is wrong, and the
distinction matters. The fourteen guarded blocks are a **complete consumer
side**: every top-level pass (shadowing, predicate returns, elide positions,
positionals, argv-splat, undeclared reads, command resolution) explicitly
handles an end block. Nothing built that and then abandoned it — the
*parser* half was simply never written, and `END { }` went out through the
`__sash_end__` function encoding instead.

So the field is not dead weight to delete; it is the other half of a design
that is 90% built.

## The catch, if it is wired up

The guards assume END is **top-level** scope — `check_no_shadow` calls
`collect_top_level_decls(end, …)`, i.e. a declaration in END becomes a
global. The `__sash_end__` encoding makes it **function** scope today:

    str x="a"
    END {
    str x="b"
    }
    -> SB5049 cannot redeclare global `x` inside a function (DD#86)

Wiring the field up therefore changes DD#86 scoping for END bodies. Which
scope END *should* have is a real question, not an implementation detail.

## Why it is blocked on DG

`:end-block` is a key in the DD#104 IR capture, a **versioned** artifact
format (`:ir-version 1`). Removing the field changes what `--emit-ir` and
`--extract ir` write.

Options:

1. **Remove the field and the key**, bump `:ir-version` to 2. Cleanest;
   costs a format version and any consumer that reads `:end-block`.
2. **Remove the field, keep emitting `:end-block nil`** as a literal.
   Format-stable, but leaves a key that can now never be anything else.
3. **Keep the field and wire it up** — make the parser populate it and drop
   the `__sash_end__` function encoding. Smaller than it first looked (the
   consumer side exists; `transform/statements.rs:159` already converts the
   function back into `exit_trap`), but it settles the DD#86 scope question
   above, and it is the only option that makes `:end-block` *truthful*
   rather than absent — and that removes the name-based coupling, under
   which a user function literally named `__sash_end__` is indistinguishable
   from an exit handler.
4. **Leave it.** 20 branches stay permanently red.

DG's coverage directive says dead code should be removed, which points at
1 or 2; the artifact-format version is why this is being asked rather than
just done.

## Related

Journal 308.
