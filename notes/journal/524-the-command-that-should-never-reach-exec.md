# 524 - The command that should never reach exec

2026-10-18. DG asked to continue coverage from `5af7a4b`: 133 compiler
production cold lines. This batch closes all eleven in `lib.rs` (7),
`regex_validate.rs` (2) and `transform/capture_purity.rs` (2).

## Four source errors, five measured lines

`cli.43` supplies an existing, non-executable `unsquashfs`. The resulting
permission-denied diagnostic must not become the missing-installation
message or a language error. `cli.44` gives the host resolver a real
two-link cycle and pins SB5095's distinguishing nonterminating-chain text.

`cli.45` directs native output to an existing directory. This reaches the
real failed-link branch, not a fabricated diagnostic, and asserts no success
stdout, no leftover object and preservation of the directory.

`rxcompile.1` passes a literal hex-NUL regex through the real CLI. It pins
SB5023's `pattern contains null byte`, not merely its shared code. The
subject variable is read, avoiding a constant-comparison refusal; `\x00`
is the supported escape rather than `\0`.

The controls first produced the intended diagnostics. Instrumenting the
actual spec files then warmed all five selected source-reachable lines.
The final CLI-only merge counts each twice, once per suite mode.

## Conservative capture answers stay answers

The purity classifier accepts a larger domain than source. A nonliteral
command head cannot be proved safe for buffering; a self-recursive or
mutually recursive function graph cannot either. Two API tests pin those
conservative false results without trying to make source pass SB5037.
Existing `cap.19`-`cap.21` already cover valid nested function captures.

Corrected the stale claim that SB5011 alone rejects every nonliteral API
shape. Flattened the function-body lookup without changing its visiting-set
semantics, satisfying the live complexity gate rather than bypassing it.

## A successful foreign compilation carries an allocation

The C producer reports every `regcomp9` compilation failure through
`regerror9`, including allocation failure. A null pointer without that flag
is not a second ordinary regex-error channel.

Removed the anonymous `regex compilation failed` answer. After checking the
flag, a private `NonNull` boundary requires the successful allocation and
names the violated protocol if it is absent. A direct should-panic API test
pins that orthogonal foreign invariant without manipulating shared C error
state. Legitimate regex-error messages and cleanup remain unchanged.

## The driver test exposed native's false fallback

The native driver's codegen-error loop calls `process::exit(1)`, so its API
test runs an isolated copy of the unit-test executable. The child supplies
an unlowered `join` command to the actual driver; the parent requires exit 1,
the input-annotated invariant diagnostic and no static-binary success
marker. There is no production fault-injection switch.

Before the repair, that child instead reached object writing and exited
101. Native had successfully generated an object for the expression
intrinsic. A second failed-before control sent `write` through the actual
native dispatcher and likewise received an ELF object instead of the
expected invariant.

The wildcard was not a builtin implementation. Its target,
`handle_general_command`, emits external exec only. C already routed all
22 unlowered builtin kinds to the shared `ice_unreachable_command`.
Native now does the same, explicitly routing only `Other` to external
emission. Its declared-total, wildcard-free match also requires a decision
when a new builtin kind is added. Corrected the related Test/Attest SSA
comments that still described legacy command fallback.

The API matrix covers all 22 kinds through 24 spellings, including
`readline`/`readarray`. The existing checked-Test assertion remains; the new
matrix uses unchecked commands. Together with the driver's Join, the shared
native refusal has union count 26 and CLI count zero. The child now reaches
the real annotated diagnostic and exit-1 path. No accepted source program
was shown to construct this malformed SSA; this fixes a regression-backstop
gap, not a newly demonstrated source-language behavior.

## Measurement and publication gates

Fresh complete-scope union: **122 / 29915** production lines remain cold,
down **133 -> 122**, with no cold-file regressions. All three selected files
are zero. Native's eleven executable modules remain **6535** production
lines, zero cold; C remains **2835**, zero cold. All **40** measured
worktree blobs are current.

Raw cargo-llvm-cov reports **99.12% line / 99.51% function / 94.36% branch**;
these include inline test scaffolding. Exactly **8774 CLI profiles** across
all **32** slots distinguish five source-warm sites from seven API-only
sites, including the repaired native guard. Missing DA records fail the
attribution gate. Saved CLI/union exports, merged profiles and the coverage
database snapshot before restoring ordinary tools.

Predicted **8917** checks: native **4561**, C **4356**, **8904 passing /
13 existing C skips**. Instrumented and ordinary sweeps match exactly.
All **8909** baseline statuses at rowids **6075764-6084672** are preserved.
The only additions are the eight intended mode-qualified IDs; no retirements
or previous-row flips. First-versus-current liveness stays **260**.

The instrumented block is **6084673-6093589**, with eight first-ever IDs;
the final ordinary block is **6093590-6102506**, with zero first-ever IDs.
Independent source plans agree with the predicted totals. Both global ID
collision checks are empty, and historical numbering checks precede the
new IDs.

Ordinary release restored with `mk cov-reset`; final units pass:
**192 sash + 27 convert + 20 expand**. Compiler and native/C joined-program
canaries produce correct output and no profiles with absent or explicit
profiling destinations. All staged gates pass without bypass. Status records
the result; no architecture or source policy changed, and no coupled finding
is deferred.
