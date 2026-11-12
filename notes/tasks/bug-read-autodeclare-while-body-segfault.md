title: "bug: an auto-declared bulk-read target inside a `while` body segfaults"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`while … { read ARR; … }` allocates its table and runs correctly"
  - "`read $s ARR` in the same position likewise"
  - "specs pin both, plus a control (explicit declaration) that was never affected"

## Found

2026-09-11, while testing the counter-argument to
`bug-while-read-array-nonterminating` — looking for the sash spelling of
bash's `while read -a arr` turned up a segfault instead.

```sh
int n=0
while (( n < 1 )) {
  (( n = n + 1 ))
  read ARR              # segfault
  echo "n=$(len ARR)"
}
```

Not `readline`-specific (a plain `while (( ))` counter does it), not
taint-related (a clean literal does it), not operand-related. `for` loops,
top level, and an explicit `array ARR=()` in the same position are all fine.

## Cause

`lower_while` (`transform/control_flow.rs`) is a two-pass lowering: pass 1
lowers the body to discover loop-carried variables, then **clears the block's
instructions** and pass 2 re-lowers with phi-merged versions. It rolled back
the instructions but not `state`.

`autodeclare_bulk_target` is guard-then-emit:

```rust
if state.collections.contains(name) { return; }
state.collections.insert(name.to_string());
```

Pass 1 emitted the empty-collection `Assign` and registered the name; the
clear discarded the `Assign`; pass 2 hit the guard and emitted nothing.
`--emit-c` showed the consequence directly — `ARR_ptr` kept its `""`
initializer and `sash_split_into_array` wrote into a read-only string literal.

In the transform, so every backend inherited it.

## Resolved 2026-09-11 (journal 265)

Snapshot `collections`/`assoc_arrays` before pass 1, restore before pass 2 —
the same save/restore `enter_fn_body_scope`/`exit_fn_body_scope` already does
for that pair. Blast radius measured by stashing the fix: DD#99's two `read`
forms and nothing else (`argv V` in the same position was already fine).

`readonly_vars` leaks the same way but is deliberately not restored — see
`while-two-pass-readonly-leak`. Version counters must not be rolled back
(`counter` is a documented high-water mark that `insert_phis_two_branch`
consumes).

Specs `commands.sh` 14.37–14.40.
