# 522 - The join that outgrew its buffer

2026-10-18. DG asked to continue coverage from `8ddded4`: 164 compiler
production cold lines, including 17 in the C emitter. All 17 are closed.
Both executable backend cohorts now have zero cold production lines.

## A cold separator led to a real output defect

The remaining `arrvals` default-space arm was API-only, but reading its
whole consumer exposed a different problem. C joined into a 4096-byte
stack buffer and silently stopped copying. Its separator check could omit
an entire separator at the boundary. Native allocated the same fixed
buffer but copied without a bound. Neither behavior implements DD#75.

The byte-exact before-controls failed six native and seven C cases. A
single 4097-byte element produced only 4096 data bytes in C; larger native
joins failed too. The source cases use the actual CLI, not a private backend
harness.

Both emitters now call `sash_table_join`. Two linear walks over live table
entries size then fill one heap allocation. The runtime shares the existing
`itoa_inner` formatter rather than allocating intermediate integer strings.
It skips deleted entries, preserves insertion order and byte lengths,
checks accumulated size, and terminates the result at `[len]`. A later join
cannot overwrite an earlier result. Heap failure uses DD#107's ordinary
diagnostic, not a successful partial result. There is no new join policy or
language limit; this removes an undocumented implementation defect.

The embedded-byte control also exposed C's greedy hex escapes: `\x00b`
encodes a single byte, not NUL followed by `b`. The canonical C string
encoder now emits three-digit octal for control bytes, which cannot consume
the following hex digit. Both the join and its input literal preserve their
bytes.

Thirteen dual-mode specs, `glob_splat.sh` `joinbuf.1`-`joinbuf.13`, cover
4095/4096/4097/8192-byte elements, an 8192-byte separator, retained result
storage, deleted integer-map cells including both i64 endpoints, embedded
NULs in values and separator, external-argv termination, empty collections,
empty elements, explicit empty separators and exact allocation failure.
The small-heap case specifically requests 40970 bytes for the joined
string, so an earlier unrelated allocation failure cannot satisfy it.

All 81 checks in the expanded file pass in both modes after the fixes.
The changed separate runtime workspace was rebuilt before C linking.

## The original C cohort

| Sites | Disposition |
|---:|---|
| 1 | Remove obsolete int-collection local-save/restore assignment fallback |
| 1 | Narrow argv helper to its already-destructured index reference |
| 1 | Remove empty parameter-operator identity; plain expansions carry no operator |
| 1 | Remove locally impossible restore when no redirect-failure metadata exists |
| 3 | Pin aggregate ownership, IntCell expression refusal and lazy logical routing |
| 4 | Pin actual non-command dispatch and three wrong builtin-helper groups |
| 2 | Pin rejected/unknown unary and binary predicate operators |
| 1 | Pin unsupported raw redirect forms after lowering |
| 2 | Refuse missing redirect slots and empty raw concatenations loudly |
| 1 | Pin retained `arrvals` default-space contract through emitted runtime call |

DD#86 retired the save/restore producer. Its temporary branch sentinel first
fired in a direct API positive control, then stayed silent across all 8877
existing CLI checks. Producer enumeration corroborated removal. Both the
sentinel and its temporary unit are gone. Pointer-storage classification
still serves collection reads; it was not removed with the dead assignment.

Three deliberately failing API controls established the old empty-operator,
missing-slot and empty-target answers before correction. Empty-target
coverage now includes both DoubleQuoted and Concat shapes. DD#119 and
native's existing refusals settle the contract. The independently filed
live `load_var_or_empty_c` tiered lookup is unchanged.

Existing non-command units now enter `emit_command_body`, not merely the
guard it is supposed to call. New same-file API tests pin only contracts
that source cannot exercise; source behavior stays in the spec suite.

Updating obsolete declaration comments triggered the live complexity gate.
Separating ordered, version-collapsed storage types from declaration
rendering keeps the same sorted C output and String-wins collection storage.
All staged gates then pass without a bypass.

## Measurement and attribution

Fresh scope-correct spec-plus-unit union: **147 / 29909** production lines
remain cold, down **164 -> 147**, with no cold-file regressions. All **40**
measured source blobs are current. Native's eleven executable modules
contain **6535** production lines, all warm; C's **2835** are all warm.
The production database excludes inline test definitions.

Raw cargo-llvm-cov reports **99.07% line / 99.55% function / 94.26% branch**.
Those headline figures include inline test scaffolding; runtime coverage
remains a separate concern.

Separate attribution merges **8760 CLI profiles**, exactly 8734 plus the
26 new source invocations, across all **32** merge slots. All twelve
CLI-cold retained sites are warm in the union. The actual C join call is
CLI-warm (58, union 59); the default `arrvals` branch is CLI 0 / union 1.
The binary/unary operator refusals are CLI 0 / union 7 and 5 respectively.

The empty-target assertion deserves a distinction: its DA record includes
the valid-input condition. It is CLI **2**, union **4**, not a CLI-cold
line. The two malformed API cases explicitly assert the failure message;
the line count alone does not distinguish condition evaluation from panic.
Missing DA records fail attribution rather than becoming implied zeros.

## Full gates and ordinary restoration

Predicted **8903** checks: native **4554**, C **4349**, with **8890 passing /
13 existing C skips**. Instrumented and restored ordinary sweeps both match.
All **8877** baseline statuses at rowids **6031295-6040171** are preserved.
The only additions are the 26 intended `joinbuf` IDs; no retirements or
previous-row flips. First-versus-current liveness stays **260**.

The instrumented block is **6049049-6057951**; the final ordinary block is
**6057952-6066854**. Its first-ever-ID count is zero, versus 26 in the first
run. Both global collision checks are empty. Independent verified file plans
sum to exactly 4554/4349; coverage.sh retains its known two-token difference
from the three-execution variable-ID loop.

Ordinary tools are restored using `mk cov-reset`. Final units pass:
**179 sash + 27 convert + 20 expand**. A persistent-directory compiler and
joined-program canary produces correct output and zero profiles, both with
profiling variables absent and with explicit compiler/runtime destinations.
GUIDE, DD#75 and status record the fixes. No coupled finding is deferred.
