# Journal 104 — ir_to_ssa_typed phase split

## Goal

Sixth Phase 11 target. `ir_to_ssa_typed`
(`transform/entry.rs:9`, CC 23). Recipe from the audit: extract
the three distinct phases bundled inside this entry function.

## Pattern

The function did four jobs in sequence with no labels:

1. Collect known function names so CommandSubstitution lowering
   can detect elidable calls (1 loop + 1 if-let).
2. Lower the top-level statements (single call).
3. Install the DD#56 `__end__` block if an EXIT trap was defined
   (1 if-let + 1 nested lower call + 1 filter + 1 collect + 1
   loop + 2 ifs + 1 conditional retarget).
4. Post-process `checked` flags on Command/Pipeline insts: DD#55
   (Branch-position → unchecked, reverse scan + break) and DD#56
   (END block → unchecked, forward scan + no break). Two passes
   sharing a `for block in ssa.blocks.values_mut()` loop with two
   independent ifs inside.

The 23 CC came almost entirely from phases 3 and 4 — both built up
deep nesting (`if let Some(stmts) { … if matches!(terminator …) { …
for inst in insts { match inst { Command{…} | Pipeline{…} | _ => …
} } }`).

## Action

Lifted each phase into a named helper. The body of `ir_to_ssa_typed`
is now linear — read it like a recipe:

```rust
state.known_functions = collect_function_names(program);

let entry = ssa.new_block("entry");
ssa.entry = entry;

let exit = lower_stmts(&program.statements, &mut ssa, &mut state, entry);
ssa.block_mut(exit).terminator = Terminator::Return;

install_end_block(&mut ssa, &mut state);
ssa.exiting_signals = state.exiting_signals.clone();
mark_unchecked_in_conditions(&mut ssa);
mark_unchecked_in_end_blocks(&mut ssa);

let diags = state.diagnostics.clone();
(ssa, diags)
```

The two `mark_unchecked_*` helpers differ in two important ways:

| | conditions (DD#55) | end blocks (DD#56) |
| --- | --- | --- |
| selector | `Branch` terminator | label starts/contains `__end__` |
| scan direction | `iter_mut().rev()` | `iter_mut()` (forward) |
| match behaviour | first match wins, `break` | mark **all** matches |

These differences matter — folding them into one helper would
hide a real semantic distinction (which inst is the condition vs
which insts are the cleanup body). Two helpers is the right shape.

The shared `match inst { Command { checked, .. } | Pipeline { checked, .. } => …, _ => {} }`
pattern collapsed both pairs of `match` arms from the original
into a single or-pattern arm in each helper. Tiny win, but it
removed 8 lines.

## Verification

```
ir_to_ssa_typed                   CC 23 → 1
install_end_block                       8  (new, DD#56 phase 1)
mark_unchecked_in_end_blocks            7  (new, DD#56 phase 2)
mark_unchecked_in_conditions            6  (new, DD#55)
collect_function_names                  3  (new)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

`ir_to_ssa_typed` drops out of the report entirely. All four
helpers stay well under 10.

## Why this was a high-leverage refactor

The 23-CC was front of the post-Phase-10 audit list (after
`check_stmt` 49 and `validate_stmt_ctx` 43), but unlike those, it
was a *linear pipeline* and so the extraction is mechanical and
the result is a clean phase reading. The named-phase pattern is
becoming the most reliable refactor in the codebase — works any
time a single function has more than one clearly-bounded
responsibility expressed as a top-level block.
