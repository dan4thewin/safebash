# 514 - The unnecessary string temporary

2026-10-16. DG pointed to `_tmp/21.c`, with its sash source in `_tmp/21`:

```sash
s=$(date +%s)
attest s ((x=s))
if (( x+=10, x>0 )) { echo yes }
echo $x
```

The emitted condition formatted the integer comparison with `sash_itoa`,
stored its pointer and length in `__arith_test`, and immediately parsed it
with `sash_atoi` to set the condition status. DG was right: that roundtrip
served no purpose.

## It was not a C-only problem

`lower_arith_test_stmt` introduced the temporary in shared SSA lowering.
`VersionState::define("__arith_test")` found no declared type and defaulted
to `String`. Both backends honored that type: arithmetic became text for the
assignment, then `LetExit` converted the text back to an integer.

The temporary had no other consumer. Rather than register a new integer
temporary or add backend peepholes, passed the lifted integer expression
directly to `LetExit`. Both emitters already accept arbitrary integer-valued
SSA operands there; `emit_for_bound` already uses exactly this form. This
also avoids injecting an unnecessary variable into version/phi bookkeeping.

Mutation hoisting and integer-function lifting stay before the status
instruction. Each backend evaluates its operand once and uses the numeric
result to set status and truth. DD#31 arithmetic, DD#55 status handling and
DD#25 overflow behavior are unchanged.

## Verify the calls, not just the output

Regenerated `_tmp/21.c`, leaving its sash source untouched. Its condition is
now a direct integer result:

```c
i64 __t3_v = ((x > 0) ? 1 : 0);
__exit_status = (__t3_v == 0) ? 1 : 0;
__test_result = (__t3_v == 0) ? 0 : 1;
```

Both the C body and native `main` went from **two `atoi` and two `itoa`
call sites to one of each**. The remaining pair is necessary: attest parses
the captured date text, and `echo $x` formats the integer for output. Checked
native function-address references against the symbol table, not just the
presence of runtime symbols in the linked binary.

Ran both versions of the exact source: each prints `yes`, then the current
epoch plus ten. The date is bounded by timestamps taken before and after
execution, not a hard-coded observation.

Added two real-CLI C-shape specs, `arithslot.1` and `arithslot.2`. They inspect
`main`, excluding the always-present extern declarations:

- An all-integer condition with literal text output emits neither conversion.
- Captured text, numeric attest, the comma condition and integer output emit
  exactly the legitimate one of each.

Both failed against the original compiler; the existing 64 CLI checks passed.
Both pass after the shared fix. These are representation assertions: ordinary
output-only tests would pass while retaining the detour. They follow the
existing mode-independent CLI-spec convention and add no unit-test rival.

Historical ID reservation and both collision checks are clean. `cli.sh`'s
plan is 66, matching its real count. Predicted full sweep: **8499** (8495 +
two dual-mode specs), native 4352 / C 4147, four new IDs, the same 13 C skips.

The ordinary full sweep matched that prediction: **8486 passing / 13 skipped /
8499 total**, exactly four new IDs, zero flips, comparison liveness 258.
Ordinary units remain **136 + 27 + 20**. Targeted CLI and arithmetic files
pass in both modes (66 and 438 checks per mode).

## Persist the fix without leaving coverage stale

Refreshed `mk cov-union` -> `mk cov-db` -> `mk cov-stale`, with no overlapping
ordinary build. The second full sweep reports zero new IDs/flips and
liveness 258. Source provenance is current.

Production cold lines stay **256**; recorded production lines drop from
29461 to **29454**. This removes seven warm lines, not seven coverage gaps.
Raw coverage remains **98.79% line / 99.23% function / 93.23% branch**.

Restored the ordinary release with `mk cov-reset`; the canary emits no
profraw. Its generated C matches the regenerated `_tmp/21.c` byte for byte.
Repeated native call-site inspection with identical `--trace` compile flags
before and after: still exactly two-to-one for each conversion. Kept the
before/after C/disassembly evidence in the session workspace and cleaned the
temporary binaries.
