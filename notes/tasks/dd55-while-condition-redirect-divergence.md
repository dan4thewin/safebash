title: "bug: native aborts on a failed redirect open in a `while` condition"
status: done
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

## DONE (2026-09-12)

Fixed, but not as a one-line `Loop` addition — the filing described one of
**three** defects, and they were in different backends.

### What was actually wrong

The filing's "second, smaller difference" (c's `i == last` vs native's
`rposition`) was not cosmetic, and there was a third defect in neither list:
both backends mistook the DD#70 `__negate_test_result` sigil for the
condition. DD#55 line 96 counts "operand of `!`" as condition position, so
`if ! cmd > /bad/f` must catch. Bash prints its diagnostic and runs the
then-branch; sash aborted, in **both** backends.

Fail-first, with the specs in place and only the source reverted:

| spec | native | c | defect |
|---|---|---|---|
| 55.67 / 55.68 `while` | fail | pass | native tested `Branch` only |
| 55.69 `until` | fail | **fail** | c's `i == last` picked the wrong inst |
| 55.70 / 55.71 `!` | fail | fail | negate sigil taken for the condition |
| 55.72 uncaught | pass | pass | regression guard, passes either way |

Three sites computed this predicate; **transform was already correct** on all
counts (`Branch | Loop`, and `uncheck_last_condition_inst` skips the sigil).
Only the two codegen backends were wrong, each in a different subset. That is
the tell: a predicate copied three times drifts three ways.

### Fix

One shared `ssa::condition_inst_index(block)`, called by both
`codegen/entry.rs` and `emit_c.rs`. Terminator must be `Branch | Loop`;
reverse scan skipping `__negate_test_result`; matches Command / Pipeline /
ReadLine / ReadLineMulti / ReadArray.

Deliberately **not** merged with transform's `uncheck_last_condition_inst`.
The two answer different questions — transform asks whether a nonzero
*status* aborts (status-bearing insts only), codegen asks whether a redirect
`open(2)` failure is caught (the reads carry their own redirects and so must
be included). Same shape, different sets; collapsing them would be wrong.

### Spec

`errors.sh` 55.67-55.72, guarded `native,c` (janet has no DD#55 abort channel
— `c-janet-dd55-abort-channel`). plan 144 → 150.

Note 55.68: `while read` is rejected outright by SB5069 ("`read` consumes the
whole stream, so the loop has no second read to make"), so the loop form of
the read family is `while readline v`, which also needs an `attest` in the
body to clear taint and DD#86. The first draft of that test used `while read`
and the compiler was right to refuse it.

### Verification

Sweep clean, zero flips vs `5d62cc1`. 55.72 pins that widening condition
position did not start swallowing genuinely uncaught failures.
