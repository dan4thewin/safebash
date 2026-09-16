# 341 — purging a pre-scan that could not insert

Sweep **5223 passing, 10 skipped, 0 failing** — unchanged, as predicted
for a pure deletion. Units 83 green.

DG: *"purge dead code; only exception would be ICE to prevent
regression."*

## Task A — the positional pre-scan (done)

`collect_positional_refs` walked every value of every instruction of
every block, every compile, to find positional (`$1`) and special
(`$#`, `$@`, `$0`) names that needed a runtime slot. Its filter ran 89
times per sweep and inserted **nothing**, because every form it looks
for is rejected long before codegen — SB5024, SB5045, SB5017 — and
in-function positionals resolve through `args` (DD#60).

The set's *other* producer, `scan_redirect_for_vars`, inserts every
redirect-target name unfiltered, and that is the live DD#53 path. So
the pass is real; only its name and four-fifths of its walk were not.

Deleted from `values.rs`: `maybe_insert_positional`,
`scan_value_for_vars`, `scan_composite_children`, `composite_children`,
`scan_test_for_vars` — **85 lines**, a closed subtree whose only
callers were each other. `setup.rs` collapsed to a redirect-only walk
and lost five dead parameters (`_externs`, `_reachable`, `_space_id`,
`_exit_status_var`, `_test_result_var`) and its
`#[allow(clippy::too_many_arguments)]`. Renamed to say what it does:
`init_undeclared_redirect_vars` / `collect_redirect_target_vars` /
`init_one_redirect_var`.

### The dedup guard was provably dead, not merely cold

`if vars.contains_key(&key) { continue; }` never fired. Instrumenting
it showed why: `VersionState::define` does `or_insert(0)` then
`*next += 1`, so **the first SSA version is always 1**. Version 0 is
this pass's private namespace, and the ref set is a `HashSet`, so a
collision cannot be constructed. Everything else that spells
`{name}_0` (`types.rs:182`, `dispatch.rs:530`) only *reads* the
convention. Gone.

The `?`/`$` guard went too — but only because journal 340 fixed the bug
hiding behind it. Before that fix it was load-bearing.

## Task B — `PreIncDec`: the filing is partly wrong

Stopped before touching it. The task file lists ~10 downstream arms as
unreachable, and they are. But two of them are **deliberately** kept,
and the codebase says so at `validate.rs:1711`:

> And even past that, a top-level `PreIncDec` classifies as a
> mutation, so `lower_arith_mutation_structured` turns it into an
> `Assignment`, never the `ArithTest` that is this function's only
> caller.

That is a defence-in-depth argument that *names* `classify_arith_expr`
(arith.rs:239) and `lower_arith_mutation_structured` (arith.rs:1292) as
the second line behind `reject_prefix_incdec`. Deleting them would
delete the backstop a comment elsewhere relies on.

Worse, most of the rest sit behind a catch-all. Removing the
`PreIncDec` alternative from `has_mutation_inner` (`_ => false`) or
`arith_has_incdec` does not produce a compile error if the invariant
ever breaks — it silently answers **"not a mutation"**. That is the
opposite of a regression guard.

Only two sites are clean deletes by the codebase's own precedent:
`brief_arith` (arith.rs:189) and `emit_ir.rs:648` are diagnostic /
serialisation formatting, and `validate.rs:2081` already documents
exactly that move for `arith_brief` — *"Were one to arrive anyway it
falls to `_` and reads as `...`"*.

So Task B is not the ~10-arm purge it was filed as. It is two safe
deletes, two documented keeps, and a handful where deleting trades a
compile-time guarantee for a silent wrong answer. Re-filed for DG
rather than decided here: the reliance is deliberate and documented,
which is exactly the case AGENTS.md says to ask about.
