title: Unattended diagnostics and signed C literals
date: 2026-11-28

# Unattended diagnostics and signed C literals

DG authorized the unattended twenty-owner session on a separate branch.
This cohort closes seven small owners using existing language contracts.
The AI implemented the diagnostics and C formatter; no new language
policy or main-branch adoption is implied.

## Changes

SB4049's examples now satisfy DD119: nonempty defaults or require
dispositions, and a capture bound before its guarded redirect. Direct
capture targets and unguarded variables retain their refusals.

Malformed arithmetic attest predicates receive their actual SB5054
diagnostic without the unrelated SB4020 arithmetic-value mutation message.
The exemption recognizes the parsed predicate rather than requiring a
successful conversion classification. The parser always constructs at
least one literal lead and an explicit predicate; malformed conversions
still have the independent SB5054 defense.

Substring validation is shared between parsing and typing. Each field is
normalized for the existing integer check, while its original spelling is
retained for an error. A valid parenthesized offset cannot mask an invalid
length. Accepted operand normalization is unchanged.

SB5012 retains the actual unavailable-state reason: no preceding command,
explicit discard, consumption, or the end of a handling scope. Its
availability/latch transitions are unchanged. Bare negative array indices
get negative-index advice instead of a string-key explanation, reusing the
existing recognizer. Dynamic negative array keys and map string keys keep
their established behavior.

The inline-glob argument check skips exactly the exec target. Its own
validator still diagnoses that operand, and a bad later argument still
gets SB5029 even when the target is also bad.

The C emitter uses `c_int_literal` for parsed integer values, arithmetic
immediates, substring offsets/lengths and the omitted slice count.
`i64::MIN` becomes `(-9223372036854775807L - 1)`, avoiding C's unsigned
decimal-constant interpretation. No runtime arithmetic miscompile was
demonstrated: the original native/C results already agreed.

## Proof and corrections

Eleven new dual-mode checks are `diagmsg.1-.10` and `cmin.1`; five existing
checks are strengthened rather than duplicated. The six affected files
pass 4288 checks. The original compiler fails exactly fifteen assertions
per mode, with all other checks in those files preserved:

| File | Original-binary failures |
|---|---|
| `arith_literals` | `cmin.1` |
| `commands` | `14.19` |
| `errors` | `55.15`, `diagmsg.2-.4` |
| `exec` | `diagmsg.9-.10` |
| `taint` | `93.42`, `93.53` |
| `types` | `sub.9`, `diagmsg.1`, `diagmsg.5-.7` |

Thirteen independent reversions produce their exact predicted failures
in both modes:

| Reverted mechanism | Failing IDs |
|---|---|
| Redirect advice | `14.19` |
| Arithmetic attest exemption | `93.42`, `93.53` |
| Original substring spelling | `sub.9`, `diagmsg.1` |
| Per-field substring validation | `diagmsg.1` |
| Ended status scope | `diagmsg.2` |
| Consumed expression status | `diagmsg.3` |
| Explicitly discarded status | `55.15` |
| Later handling-body statement | `diagmsg.4` |
| Negative-index advice | `diagmsg.5-.7` |
| Exec-target separation | `diagmsg.9-.10` |
| Parsed C integer literals | `cmin.1` |
| C substring offset | `cmin.1` |
| C substring length | `cmin.1` |

The substring field control catches an implementation mistake made during
this work: retaining the whole raw operand and then validating raw fields
could blame the valid `(-2)` offset instead of the invalid length. The
shared field validator now normalizes for checking but returns the original
invalid field.

One attempted C counter-immediate reversion did not fail. The full native
arithmetic-literal file passed, including its explicit C/GCC check. Source
enumeration explains why: the four `SsaValue::ArithExpr` constructors are
synthesized loop increments with operand `1`. Ordinary source arithmetic
uses `ArithBinOp`, whose operands take the parsed-literal route. The failed
control is not evidence for a minimum immediate in `ArithExpr`; no C-mode
run of that campaign is claimed. The formatter call remains for uniform
emission, not a fabricated source-coverage claim.

`cmin.1` checks values, a legally handled status-function return, unfolded
addition/multiplication and substring operands. It compiles emitted C
with `gcc -std=c11 -Wall` and rejects integer-constant warnings, not
unrelated existing unused-label/variable warnings. Even its native-mode
run performs this C check. An early private result checker incorrectly
searched TAP sequence numbers for spec IDs; TSV accounting recovered the
completed runs without rerunning or treating that mistake as a product bug.

## Ordinary gates

The first full gate passes with
`NEW=22 FLIPS=0 RETIRED=0 TOTAL=13529`, rows 7651519-7665047.
It was run while the two source-inert counter/MAX formatter calls were
being considered for removal. Restoring the uniform formatter calls
returns production source and the rebuilt compiler to the original
positive identities. A second gate on that restored implementation passes
with `NEW=0 FLIPS=0 RETIRED=0 TOTAL=13529`, rows 7665048-7678576.
Only the explanatory state-machine comment was corrected afterwards.

Both sweeps contain native 6891 and C 6638 checks: 13516 passing and
13 existing skips, no failures, flips, retirements, ghosts or plan
mismatches. Liveness is 246. All 13507 converter-cohort IDs retain their
statuses, and exactly 22 IDs are added. The gates include ordinary build
and unit checks; formatting, Clippy and static checks pass. No runtime,
dependency or coverage change.

Private evidence is in `files/batch20-20261128/`: the frozen before and
positive compilers, before/after TSVs, thirteen effective reversion
families, the counter-control result, `diag-gate.log`,
`diag-gate-restored.log`, restoration identities and xtraces.

## Deferred owner and remaining work

The SB5041 arithmetic-wording item is not a small wording fix under the
current contract. Forward command and arithmetic uses still reject, but
DD81/DD86 require a unique hoisted definition independent of placement.
The old define-before-use remedy is superseded. Its existing task now
requires separately scoped forward-resolution repair and migration of
old SB5041 assertions. No new policy question or compiler change is hidden
inside that deferral.

Eighteen selected owners are complete, one is deferred, and the
complexity-index availability owner remains. Work stays on
`unattended-backlog-20261128`; main, the reference worktrees and unrelated
user work are preserved.
