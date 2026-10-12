# 502 — Two tautologies and a wrapper nobody tested

Continuing the cold-line queue, now at 316 lines after journal 501.

## `parse_trailing_pipeline` — three cold lines, one cause

Three cold lines in one function usually means three questions. Here it
meant one, asked twice:

```
2880  if !matches!(scanner.peek().token, Tok::Pipe) { return Ok(stmt); }
...
2890  if matches!(scanner.peek().token, Tok::Pipe) {      // always true
...
2909  if commands.len() > 1 { … }                         // always true
2913  Ok(if commands.len() > 1 { Pipeline } else { … })   // else dead
```

Line 2890 re-tests the token line 2880 already returned on, and nothing
between them advances the scanner. The `commands.len() > 1` pair is dead
for a different reason: `commands` starts at one stage and
`append_pipeline_stage` pushes or extends on all three of its arms, so
the list always grows. The counts corroborate — 454 entries, 446 reaching
the append, 416 taking the `> 1` branch, zero taking the else.

The invariant needs the empty-`Pipeline` case ruled out too, so I
enumerated the construction sites rather than assuming. There are exactly
two in the parser: this one (needs two stages) and `parse_negated`'s
one-command wrapper. Neither can build an empty one.

Both conditions removed. The comment at 2885 had already recorded that
this was a `while` once and that the fall-through was the SB4056 bug —
the conversion to `if` left the condition behind.

Sweep unchanged at 8359, zero flips. The three lines are gone rather than
warmed, which is the better outcome when a line is unreachable *within
its own function*: AGENTS.md says not to chase those with a test, and
deleting the question is the one move that leaves nothing to chase.

## `compile_to_object` — the wrapper three tests did not test

`codegen/entry.rs` already had three unit tests for
`panic_payload_message`, the decoder that turns a caught `catch_unwind`
payload into a diagnostic string. All three were warm. Line 74 — the
`Err(payload) =>` arm in `compile_to_object` that *calls* it — was cold.

That gap is exactly DG's orthogonality test. Delete the `catch_unwind`
from `compile_to_object` and all three existing tests still pass, because
none of them calls `compile_to_object`; the ICE would just escape as a
stacktrace, which is the single thing the wrapper exists to prevent. The
decoder and the wiring are different claims and only one was held.

The shape that drives it: an `SsaProgram` whose `entry` names a block
that does not exist. `ir_to_ssa` cannot produce one — every entry id
comes from `new_block`, which inserts before it returns — so no source
program reaches this and no spec ever could. `SsaProgram::new()` is
already exactly that program, which made the test four lines plus a
`set_hook` to keep the expected backtrace off stderr.

`mk test` 118 → 119.

## Where the queue goes next

316 → 312 cold lines. By area the remainder is:

| area | cold |
|---|---|
| `codegen/` | 65 |
| `validate.rs` | 42 |
| `emit_c.rs` | 37 |
| `type_check.rs` | 25 |
| `parse/parser.rs` | 23 |

**`codegen/` is the largest cohort and it is blocked on one thing:**
there is no scaffolding for calling an emitter directly. Every candidate
there (`emit_attest`, `load_var_or_empty`, `lower_cmdsubst_as_int`,
`emit_arith_op`, `emit_redirect_push`, …) needs a `FunctionBuilder`, an
`ObjectModule` and an `ExternFuncs` before it can be called at all. A
shared `#[cfg(test)]` harness is one piece of work that unlocks the
whole cohort, and it is the obvious next investment.

One finding to raise with DG before building on it, though:
`load_var_or_empty` returns **(null, 0) silently when a variable has no
codegen slot**, and its doc justifies that with *bash* semantics
("Bash treats undefined vars as empty strings"). sash's own declaration
rules make that justification stale, and journal 449 is the session where
a silently-missing slot was the bug. `emit_attest` has the same shape.
Whether those should become ICEs is a behaviour question, not a coverage
one.
