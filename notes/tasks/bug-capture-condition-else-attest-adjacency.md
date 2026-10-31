---
title: "Capture-condition else drops attest adjacency"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "An adjacent attest in a capture-condition's else branch compiles and validates the captured output"
  - "Single-command and pipeline capture conditions work in both native and C modes"
  - "Inherited taint and genuinely non-adjacent attests remain rejected"
---

# Capture conditions lose attest adjacency in `else`

Found 2026-10-31 while building the external C++ runtime's fifth program
pair. This is a native validator false positive, not a C++ runtime defect
or a request to change the language.

[DD#88](../decisions/DD88-capture-condition.md) binds stdout even when the
captured command fails. [DD#121](../decisions/DD121-attest-adjacency.md)
explicitly counts an `if` condition as the tainting operation immediately
above its branches. The first `attest` in either branch must therefore
receive the condition's appropriate adjacency information.

The validator supplies that information to `then` but discards it for
`else`, rejecting valid failure handling with SB5088.

## Minimal reproducer

```sash
if captured=$(/bin/false) {
    attest captured [[ $captured == '' ]]
    echo unexpected
} else {
    attest captured [[ $captured == '' ]]
    echo caught
}
```

Expected: compile successfully and print `caught`, exiting zero.
`/bin/false` produces no stdout; the failure branch's literal empty-string
predicate succeeds.

Actual: compilation fails at the `else` branch's first statement:

```text
5:5: error [SB5088] attest of 'captured' is not adjacent to the operation that tainted it
```

Replacing the condition with
`captured=$(/bin/false | /bin/cat)` produces the same rejection. Both
forms were reproduced through the real supplied sash CLI.

The corresponding `then` control compiles and prints `success`:

```sash
if captured=$(/bin/true) {
    attest captured [[ $captured == '' ]]
    echo success
} else {
    echo unexpected
}
```

## Where the change belongs

Start with the conditional taint walker and attestation-adjacency gate in
`sash/src/validate.rs`, currently `taint_if`, `taint_block`, `block_taints`,
and `check_adjacency`. Names and locations may move; recognize the code
that clones branch taint states, constructs the condition's `JustTainted`
seed, and joins fall-through paths.

The current asymmetry is:

```rust
let body_lead = block_taints(cond, sx);
taint_block(body, &mut body_out, &body_lead, sx, diags);
// ...
taint_block(eb, &mut else_out, &JustTainted::new(), sx, diags);
```

`check_adjacency` correctly rejects a tainted subject absent from
`JustTainted`; the `else` caller has thrown away the information that
makes this subject adjacent. Correct the branch seed, not the gate.
Reuse the existing condition-taint mechanism rather than introducing a
second expression walk.

## Constraints

- Preserve DD#121 R1, branch-local cleanliness, and fall-through joins.
  Do not seed adjacency from the entire inherited tainted set.
- Account for `elif`: adjacency belongs to the condition immediately
  preceding the branch. Do not make an earlier condition's taint adjacent
  across an intervening condition.
- Keep DD#88's status consumption and captured bytes unchanged. This fix
  is attestation eligibility, not capture execution or aggregation.
- Do not close this task by changing the reproducer to `${captured:!}`,
  discarding its value, or weakening SB5088 generally.

## Regression coverage and completion

Add focused CLI-driven specs to the existing capture-condition/taint
suite, not a new Rust integration harness:

| Case | Expected |
|---|---|
| Failed single external capture, first `else` statement attests its output | Compiles; prints the validated failure-branch output |
| Failed external pipeline capture, same `else` attest | Compiles; preserves the failing aggregate and captured bytes |
| Successful capture with adjacent `then` attest | Existing behavior remains unchanged |
| Final failing `elif` captures the value attested first in `else` | Adjacent attest accepted |
| Value tainted before the `if`, with no condition-owned taint of that value | In-branch attest remains SB5088 |
| An ordinary statement separates capture-condition entry and `else` attest | Remains SB5088 |
| Only an earlier condition taints the value; another condition intervenes before `else` | Remains SB5088 |

Include nonempty output from a failing producer so the positive does not
only prove the empty-string case. Assert successful CLI compilation and
the actual output; negative specs must name SB5088.

Use collision-free spec IDs, including retired IDs recorded in `spec.db`.
Predict the new check count before running both modes. Complete `mk test`
and the full spec/regression gates. Reverting only the corrected `else`
adjacency seed must turn the new positive specs red while the `then`
control stays green.

This differs from
[the non-caught-position wording task](sb5088-wording-for-non-caught-attest-positions.md):
those forms are correctly rejected with misleading wording; this form
is explicitly allowed and must compile.
