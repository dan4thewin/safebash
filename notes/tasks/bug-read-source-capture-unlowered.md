title: "Command substitutions in read sources reach SSA and codegen unlowered"
status: done
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

## Cause and implemented repair, 2026-11-28

Sed sugar carries an implicit boolean-discarding `gsub` substitution.
`capture_value` (`sash/src/transform/expr_handlers.rs`) represents
that suppressed substitution as `SsaValue::CmdSubstBody`, pending normal
materialization.

At the repair baseline `fa55ed7e`, `lower_newread`
(`sash/src/transform/cmd_handlers.rs`) converted the source with
`expr_to_value`, but invoked `lift_int_func_calls` only when an attached
redirect scope was nonempty. Without that scope, the unlowered body was
placed directly in `SplitValue`. The SSA backstop classified the carrier
as tainted; disabling taint merely let it reach the backend's
unlowered-value guard.

Every explicit read source now passes through the shared unsuppressed
materializer. Only the extra scalar snapshot remains conditional on an
attached redirect scope. The repair adds no walker, taint exception,
runtime interface or caller attest/forced read, and leaves splitting and
collection publication unchanged.

DG selected this repair from the backlog on 2026-11-28. Five real-CLI
regressions, `readsrc.1`-`.5` in `spec/wordsplit.sh`, all fail on the original
compiler and on the final-tree reversion in both modes; all 152 cases pass
after restoring the repair. Separate before/after controls preserve source
evaluation before executable redirect targets and SB5020 for tainted
captures. The predicted full gate passes with ten new mode IDs, no flips or
retirements, and 13,426 checks (13,413 passing, 13 existing skips).
[Journal 680](../journal/680-computed-read-sources.md) records the proof.

The [negated-test buffering limitation](q-buffer-capture-eligibility.md)
remains separate. In the original resolver, moving `!` inside `[[ ]]` removed
the outer capture's SB5020 but exposed six downstream backstop diagnostics
inside the function. The reduced program above has no negated test and
isolates the repaired source-lowering defect.

The original 2026-11-24 request was backlog recording, not implementation.
[Journal 669](../journal/669-capture-buffer-and-read-source-backlog.md)
retains that evidence and scope. This repair did not read or execute the
operational script.
