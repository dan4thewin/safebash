title: "Command substitutions in read sources reach SSA and codegen unlowered"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "The reproducer compiles with normal taint checking and prints warm in both native and C modes, without a backstop diagnostic or ICE."
  - "Materialize the read source exactly once through the shared value-lowering machinery; preserve operand/body failure boundaries, regex-result discard, and actual source taint."
  - "Preserve plain value splitting, NUL-separated fields, target publication, and attached-redirect evaluation order; cover the affected source forms with focused real-CLI specs."
  - "Prove the new regression on the pre-change compiler and with a reversion of the repaired source-lowering path."

## Evidence, 2026-11-24

DG encountered this in `resolve_reboot_type`, `_tmp/t1.sb:320`. An
independent reduced program needs no external commands or target-host tools:

```sh
fn f { read ${1:s/,/$'\x00'/g} _a; for entry in @_a { echo $entry } }
s=$(f warm)
echo $s
```

At `54b49dd063cbcbd15d651b93223a08292394f7b6`, both native compilation and
C emission reject it with the normal frontend enabled:

```text
tainted variable entry_8 used as an argument to 'echo' without attest (SSA taint backstop)
```

The literal argument is clean. Replacing the source expression with `$1`
compiles, as does echoing the substitution directly rather than splitting it.
With `--no-taint`, both backends instead reach the invariant failure:

```text
CmdSubstBody should have been lifted during transform.
```

The native CLI reports failure with exit 1; C emission panics with exit 101.
These were compile-only controls, not executions of the operational script.

## Cause and repair boundary

Sed sugar carries an implicit boolean-discarding `gsub` substitution.
`capture_value` (`sash/src/transform/expr_handlers.rs:95-118`) represents
that suppressed substitution as `SsaValue::CmdSubstBody`, pending normal
materialization.

`lower_newread` (`sash/src/transform/cmd_handlers.rs:39-73`) converts the
source with `expr_to_value`, but invokes `lift_int_func_calls` only when an
attached redirect scope is nonempty. Without that scope, the unlowered body
is placed directly in `SplitValue`. The SSA backstop classifies the carrier
as tainted (`sash/src/ssa_taint.rs:1089-1104`); disabling taint merely lets
it reach the backend's unlowered-value guard.

Repair the missing source-lowering step, not the symptom by making an
unlowered body clean or adding an attest/trust escape to the caller.
Reuse the existing shared materializer; no new walker, taint exception,
runtime interface or diagnostic policy is selected by this filing.

The [negated-test buffering limitation](q-buffer-capture-eligibility.md)
is separate. In the original resolver, moving `!` inside `[[ ]]` removes
the outer capture's SB5020 but exposes six downstream backstop diagnostics
inside the function. The reduced program above has no negated test and
still fails.

DG requested backlog recording, not implementation.
[Journal 669](../journal/669-capture-buffer-and-read-source-backlog.md)
records the scope.
