# 521 - The file that was not a test

2026-10-18. DG asked to continue coverage from `a5211f8`. The initial
measurement reported 175 compiler production cold lines, including eight
remaining native sites. Those eight are closed, but the final scope audit
found that the measurement itself omitted the production `[[ ]]` emitter.

## Echo separators belong to runtime arguments

Both backends printed a leading space for:

```sh
str x=$(getenv SASH_ECHO_EMPTY)
echo ${x:!~} tail
```

With an empty environment value, DD#84 says the first argument contributes
zero tokens, hence no separator. The old source-index test still counted it.
A real empty argument, `${x:!}`, must continue to count.

Native echo now carries both accumulated write failure and runtime argument
presence through elision joins. C tracks the same state. Present arguments
count even if empty or their write fails; skipped elisions change neither
field. Echo keeps its raw writes, including interior NUL bytes, instead of
borrowing the external argv builder's different contract.

The first backend fixes left two capture regressions per mode. Single echo
captures used the scalar concatenation shortcut, where `ElideEmpty` cannot
appear. Captures containing elision now retain the original command body,
including its suppression wrapper; fixed-size echo captures keep the shortcut.
Actual syscall tracing shows pure echo body capture remains in-process.
A pipeline's observed successful fork provides the positive tracing control.

Fourteen dual-mode specs, `84.73`-`84.86`, cover skipped and kept values,
multiple elisions, real empty tokens, the exact all-elided newline, pipeline
and both capture routes, and a one-write-only EIO on each elision join edge.
The latter proves later successful writes cannot erase an earlier failure.
DD#84's explicit integer-zero deferral remains unchanged: `"0"` is kept.

## Empty artifacts are not unreadable artifacts

Empty source compiled and ran, but source and manifest extraction failed:
the linker drops both zero-length sections. The mandatory nonempty IR now
identifies that output. Source-bearing older binaries remain recognizable.
Only absent source/manifest sections can be genuinely empty; missing IR and
ordinary ELF files still fail.

The old manifest fallback also masked present unreadable data as empty, and
the common reader printed `<binary data>` as successful extraction for
invalid UTF-8. Both now report explicit section-specific errors.

DD#109 requires all three artifact texts. The codegen bundle therefore
requires three string references rather than carrying unused optional
omissions; the literal interner's unused source argument is gone too.
DD#109 and GUIDE record the extraction distinction.

Five dual-mode specs, `109.8`-`109.12`, pin actual zero-byte extracted files,
byte-equal empty-program IR, rejection of ordinary ELF, and unreadable/UTF-8
corruption matrices over all three artifacts. The fixture verifies setup and
checks that all three matrix iterations ran.

## The selected native contracts

Redirect literals and branch-trace literals now use the canonical literal
lookup/lowering path rather than duplicating refusals or silently dropping
trace output. The existing missing-literal assertion pins its exact claim.

A reachable unbound `fn_` template cannot safely be skipped. It now reports
the specific lowering invariant instead of leaving a declared block unfilled.
Direct API assertions also pin nonscalar redirect targets and undefined
collection context. These are backend contracts, not rival source tests.

Three touched capture functions initially failed the cognitive gate.
Flattening their control flow and separating capture-specific intrinsic
selection brought them within the live thresholds; no bypass was used.

## The coverage filter hid production code twice

The final source-module inventory found `codegen/mod.rs` unconditionally
includes `codegen/tests.rs`: 381 lines of production test-expression emission.
All three exporters excluded it through a blanket `tests.rs` regex.
The earlier native-zero claim applied only to that incomplete scope and was
withdrawn pending measurement.

Narrowing our regex was not enough. A new exported-source canary rejected
the supposedly corrected full export too. Verbose reporting showed that
cargo-llvm-cov appends its own default filename exclusion for `tests.rs`.
An unfiltered covmap export contained the file, proving this was an exporter
policy fault rather than absent instrumentation.

`contrib/cov-common.sh` now supplies all three exporters with explicit
report arguments, disabling cargo-llvm-cov's default filename filter.
`parse/tests.rs` remains excluded; production `codegen/tests.rs` does not.
Every lcov export must contain the production module's source record.
The guard rejected the retained old export with its distinguishing error;
the full, probe and batch exports all accept the corrected scope.

The corrected measurement exposed four production cold lines:

| Site | Classification and coverage |
|---|---|
| `-G` file-test opcode | Source gap: dual-mode `ft.47` checks effective-group ownership and a missing path |
| Unary operator refusal | API invariant: four rejected unary operators plus an unknown operator |
| Shared file-opcode lookup's `None` | Exercised by that same unary API matrix, not another unit |
| Binary operator refusal | API invariant: all six rejected numeric operators plus an unknown operator |

Producer enumeration confirms authored tests retain their validated
operators, conversion produces `i?`, dispatch produces exact/regex tests,
and capture lifting preserves the operator. The guards are downstream SSA
checks, not duplicate validation on the same AST.

Two same-file API units close the invariant sites. The source spec comes
first and uses the real CLI in both backends. No test-emission behavior was
changed. Stale comments claiming `-t` remained supported were corrected.

## Corrected measurement and attribution

The final serialized spec-plus-unit union and database record **40** current
worktree blobs and **30123** production lines. The formerly omitted emitter
adds **268** measured production lines; all are warm. All eleven executable
native modules are represented, with **6698** production lines and **zero**
cold lines.

The originally measured cohort falls **175 -> 164**, with no cold-file
regressions. Separately, the newly visible emitter falls **4 -> 0**. The
corrected final compiler-wide count is **164**; the incomplete 175-line
baseline is not a complete-scope baseline.

Raw cargo-llvm-cov coverage is **99.06% line / 99.54% function / 94.20%
branch**. That report includes inline test scaffolding; the production
database excludes lines defined inside those modules. Its function headline
is not directly comparable to the earlier scope that hid this whole file.

Separate attribution merges **8734 CLI profiles** for the compiler signature
across all **32** merge slots, excluding the unit executable. Missing DA
records are errors, never implied zeros.

| Site | CLI | Union |
|---|---:|---:|
| Reachable unbound template | 0 | 1 |
| Nonscalar redirect | 0 | 2 |
| Undefined collection | 0 | 1 |
| Canonical missing literal | 0 | 1 |
| Genuine empty-artifact extraction | 8 | 8 |
| Echo-elision body-capture fallback | 6 | 6 |
| Unary test refusal | 0 | 5 |
| Shared file-opcode lookup's `None` | 0 | 5 |
| `-G` opcode | 4 | 4 |
| Binary test refusal | 0 | 7 |

The probe integration separately observes the previously excluded `-G`
line with count 2. Batch integration runs the same 47 source checks and
accepts the corrected scope; it is an export-wiring check, not a claim that
an empty warming report proves unreachable code.

## Final gates and ordinary tools

The predicted sweep is **8877**: native **4541**, C **4336**, totaling
**8864 passing / 13 existing C skips**. Both corrected instrumented and
restored ordinary sweeps match. All **8837** baseline statuses, saved at
rowid **5978042**, remain unchanged; the additional **40** IDs are exactly
28 echo, 10 artifact, and two group-test checks.

The corrected instrumented sweep adds the final two IDs; the ordinary
rerun adds none. No retirements or previous-row flips. First-versus-current
history liveness remains **260**, proving the comparison can report changes.
The final ordinary sweep occupies rowids **6031295-6040171**.

Both global ID-collision checks are empty. Independent per-file plan totals
match 4541/4336; the sole lexical mismatch remains coverage.sh's known
three-execution variable-ID loop.

Ordinary release tools were rebuilt with `mk cov-reset`. Final `mk test`
passes **170 sash + 27 convert + 20 expand** units. A persistent-directory
compiler/program canary produces correct output and no profiles, with and
without explicit `LLVM_PROFILE_FILE` and with runtime profiling configured.
Source provenance remains current after restoration.

The final staged hook checks formatting, clippy with tests, totality and
touched-function complexity without a bypass. AGENTS documents the repaired
scope gate. The owned empty-source executable and spec-mode corpus symlink
are removed. No coupled finding is deferred.
