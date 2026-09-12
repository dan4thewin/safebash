status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`while echo hi > /nonexistent/f { … }` catches in native, as it already does in c"
  - "the loop condition is false, the body does not run, rc is 0, and nothing is printed"
  - "a spec pins the `while` form beside the existing `if` and `||` ones (55.60-55.64)"

# bug: native aborts on a failed redirect open in a `while` condition

```sh
int n = 0
while echo hi > /nonexistent_dir_55/f {
  n = $(( n + 1 ))
  if (( n > 2 )) { break }
}
echo "done n=$n"
```

| | result |
|---|---|
| **native** | `<argv0>: /nonexistent_dir_55/f: No such file or directory`, **rc 1** |
| **c** | `done n=0`, rc 0 |
| bash | diagnostic, then `done n=0`, rc 0 |

c is correct and native is wrong.

## This is settled, not a design question

It was originally filed as a DD#55 behaviour question for DG. It is not one —
DD#55 already answers it twice:

- line 39 tabulates `cmd || action` / **`if cmd`** / **`while cmd`** together
  as "Explicit handling — `$?` available in handling position";
- line 96 defines **condition position** as "`if` / `while` / `until` test,
  operand of `!`".

`while` is a condition position, so a failed open there is caught. The `if`
form already behaves that way in both backends (verified), and specs
55.60-55.64 pin it for the read family. `while` was simply missed.

## The silence is deliberate — do not "fix" it

sash prints nothing on a *caught* open failure where bash prints its
diagnostic. That divergence is intentional and shipped: the accepted gates on
`abort-diagnostics-remaining` include

> "diagnostics fire only on the actual abort — caught (if/||) and
> `-`-suppressed stay silent"

So the target behaviour is c's exactly — catch, stay silent, rc 0.

## Mechanism

The two backends compute `in_condition` from different terminator sets:

| backend | site | test |
|---|---|---|
| native | `codegen/entry.rs::emit_block_insts:293` | `Terminator::Branch { .. }` |
| c | `emit_c.rs:660` | `Terminator::Branch { .. } \| Terminator::Loop { .. }` |

Native never treats a `Loop`-terminated block as a condition, so the guard in
`emit_redirect_guarded_body` takes the abort path instead of the catch path.

A second, smaller difference sits next to it and should be checked while
there: c takes the last *instruction* of the block (`i == last`), native takes
the last Command/Pipeline/read by `rposition`. They agree on every case tested
so far, but they are not the same rule.

## Found

2026-09-11, fixing `read-family-redirect-open-not-guarded`. Independent of it:
reproduces at `c1e9807` with a plain `echo` and no reads at all.
