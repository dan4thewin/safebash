# 643 - One diagnostic for rejected eval

DG noticed that the tour's `eval` example displayed two errors rather than
one. The AI traced the extra error to the compiler and repaired that path;
the authored snippet and the generator are unchanged.

## The rejection did not end command lookup

Both errors came from structural validation. `validate_command` emitted
SB4001 through `check_dynexec`, then the same batch's `check_commands`
treated `eval` as an unresolved external and added SB5011. The later
command-resolution pass was never reached. SB5011's suggested remedy, an
absolute path or a function definition, was misleading for this forbidden head.

`check_dynexec` now reports whether it diagnosed the head, and
`check_command_head` calls it before generic lookup. A diagnosed head
returns there. The single existing classifier still owns `eval`, `source`,
`.`, `command` and `coproc`; neither the builtin whitelist nor another
forbidden-name table is involved. Codes, message distinctions and spans stay.

This is a local return, not a filter over all diagnostics. The shared
traversal still visits command-substitution operands, function bodies and
sibling commands. Defining a function named `eval` still cannot make its
invocation legal. No new language rule, runtime change or dependency is
introduced.

## Focused evidence

Four existing assertions, `cov.11`, `cov.22`, `cov.58` and `cov.99`, now
require exactly one diagnostic as well as SB4001. Their source programs are
unchanged. Two new dual-mode checks cover the dependent boundaries:

- `rejecthead.1`: suppressed/redirected `eval` in a function body and
  `coproc` in a capture each retain their own SB4001, with no lookup cascade.
- `rejecthead.2`: a missing command inside `eval`'s operand and a missing
  sibling command both retain SB5011; only the outer `eval` skips lookup.

All six assertions failed on the unchanged `bb48c4ca` compiler in both
modes; the other 368 checks passed. All 374 passed after the repair.
Disabling only the new early return, while still emitting SB4001, restored
exactly those six failures per mode. Restoring and rebuilding reproduced
the positive source/compiler identities before the final gate.

The first private result checker mistakenly expected a failing spec file
to exit zero. `common.sh::tally` exits one; the sweep's `run.sh` does not
propagate that test failure. The saved native results were validated against
their TSV and trace without rerunning them, then the C control was completed.

`mk -p 1 tour-doc` executed all 56 snippets with no expectation mismatch.
The generated README differs by exactly the removal of the extra SB5011
line. The exact two-line tour example also reports only SB4001 at line 2.
Authored tour, runner/configuration, runtime archive, companion binaries and
both local lockfiles remain byte-identical.

## Ordinary gate

```sh
NEW=4 FLIPS=0 RETIRED=0 TOTAL=11631 timeout 1200 mk -p 1 gate
```

The first sweep passed: rows **7016894-7028524**, **11618 passing and 13
existing skips**, native **5927** / C **5704**. All **11627** preceding
regex-gate statuses are retained unchanged. Exactly the four new mode IDs
were first-ever; there were no failures, flips, retirements, ghosts or
plan mismatches. Liveness is 251. Rust tests pass at **239 / 27 / 20**.

Formatting and the touched functions' live complexity checks pass.
No coverage run or unrelated backlog implementation was undertaken.
Session evidence is in `files/tour-eval-20261117/`: baseline/positive
identities, actual before/positive/reversion TSVs and traces, tour output
comparison and the ordinary gate log.
