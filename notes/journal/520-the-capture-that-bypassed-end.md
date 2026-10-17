# 520 - The capture that bypassed END

2026-10-17. DG asked to continue coverage from `32e6892`, with 181 compiler
production cold lines. Selected the four remaining native value-handler
sites: slot-length dispatch, two external-capture closing edges, and the
direct integer status emitter.

## A second process builder was still bypassing the error model

Executable controls showed that an ordinary failed external capture skipped
END in both backends. Native discarded pipe failure, could run the child
with uncaptured stdout, and passed a failed fork's `-1` to wait. C returned
successful empty captures on pipe/fork failure. Child stdout duplication
was unchecked too. These were compiler bugs, not unexercised closing braces.

Single unredirected external captures now become one-stage SSA pipelines.
`PipelineCapture` distinguishes no capture, lastpipe's published reader, and
a private string target. Both backends initialize that target, check
construction through the existing pipeline machinery, drain before waiting,
and route failure through the existing caught/suppressed/END paths.

This applies DG's settled construction policy: close successful pipes and
wait for started children, with no forced kill or kill timeout. Only
lastpipe capture publishes or invalidates the ambient reader. An inner
private capture, or an ordinary failed pipeline, must not lose the outer
loop's descriptor.

The old capture path evaluated arguments in the parent. Ordinary pipeline
stages evaluate them in the child, so merely reusing that builder would have
changed `getpid`, parent-owned `wait`, and argument side effects when pipe
creation fails. Value captures therefore prepare argv before pipe/fork;
other pipeline modes retain their established evaluation order.

The new definition is wired through native base-slot/version mapping,
literal interning, C declaration collection and SSA taint. Capture targets
use the existing reserved-name mechanism, generalized from loop integers
to typed temporaries; authored later bindings are reserved too. Deleted the
duplicate backend capture emitters and unused runtime `sash_capture_status`.

## Collection length was not a scalar request

The old transform selected `arrlen` when collection membership was known,
otherwise `strlen`. The latter backend operator merely read the slot's
length field; scalar byte length already has `SsaValue::StrLen`.

A temporary producer sentinel was not cold: it failed 33 checks per backend.
An initial commentary claiming the sweep was clean was wrong; querying the
database corrected it. The backtrace led to unbound function templates,
whose implicit `args` and collection parameters do not yet have bound
collection membership.

A second sentinel excluded templates and still failed five checks per
backend: `75.73` and `ud.22`-`ud.25`. Those operands are rejected by later
diagnostics. Transformation visits them before all diagnostics finish, so
panicking in this producer would break valid template lowering and ordinary
error reporting. Both temporary sentinels and their probe unit are gone.

DG's completed 2026-09-10 amendment settles the actual contract: "make len
collection only, add strlen for scalar." `len` now always constructs
`ParamExpansion(arrlen)`; scalar length remains `StrLen`. Removed the
obsolete slot-length operator from both backends and SSA measurement
recognition. Both existing retired-operator API matrices failed before the
backend removal and pass afterward. DD#75 now records the amendment rather
than presenting its superseded polymorphic rule as current.

## The remaining API claims

Source `status` normally lowers to its managed slot. Retained the direct
native integer emitter and added an API assertion that snapshots 37,
rebinds the slot to 42, and verifies the original SSA value survives with no
calls in the generated function. C already had its own snapshot assertion.

String `CmdSubst` dispatch now accepts intrinsic carriers only. Native and C
API matrices reject empty argv, a nonliteral head, an unlifted absolute
external, and an unknown bare intrinsic. These assertions deliberately
exercise backend contracts, not a rival source-language harness.

The first fresh union exposed one newly cold taint arm: basename `echo`.
Producer enumeration shows builtin echo becomes concat/body capture, while
every absolute external head now becomes a Pipeline value definition.
Neither produces an echo carrier at SSA taint.

The stale arm also classified a malformed `/bin/echo` API carrier as Clean
when its arguments were clean. Extending the existing API matrix with that
case failed before removal. Deleted the obsolete pass-through classification
and unused taint-map parameter; unknown external carriers now remain tainted
before backend refusal. No new source spelling or taint exception.

The same basename normalization could lend generator semantics to an
external `/bin/getpid` or `/tmp/status`. Those extensions to the API matrix
failed too. Classification now matches the exact bare intrinsic name, not
the basename; paths cannot inherit a managed generator's Clean result.

## Source regressions and documentation

Added 24 dual-mode specs, `capext.1`-`capext.24`, bringing commands to 217 per
mode. They pin END routing; checked, suppressed and caught pipe/fork failure;
actual child dup2 failure; 131072-byte binary output; trailing-newline
stripping; parent argument evaluation; temporary-name collisions; nested
lastpipe preservation; nested function capture; FIFO stdout; and continuation
after failed captures inside END.

Child setup injection uses `strace -f` and positively asserts the injected
dup2, rather than tracing only the parent. The argument-side wait control
also proves evaluation precedes a failed pipe construction.

DD#56's abort-on-cleanup-failure prose was stale. Specs 56.80-56.87 explicitly
attribute "diagnose, restore, keep cleaning up" to DG, and journal 367 records
the existing unchecked cleanup mechanism. Corrected that prose; the new
pipeline captures follow the established cleanup policy. DD#55 and GUIDE
describe the shared process builder and private-reader distinction.

## Measurement

Fresh serialized spec-plus-unit union, database load and provenance check:

| Cohort | Before | After |
|---|---:|---:|
| Native `codegen/value_handlers.rs` cold production lines | 4 | 0 |
| C emitter | 20 | 18 |
| Compiler total | 181 | 175 |

No file gains cold lines, and earlier zeroed cohorts remain zero. The
database records **29823** production lines. Raw cargo-llvm-cov coverage is
**99.03% line / 99.50% function / 94.05% branch**. All 39 measured worktree
blobs match.

Separate attribution merges **8672 CLI profiles** for the compiler
signature across all **32** merge slots, excluding the unit binary.
Missing DA records are errors, not zeroes.

| API-only site | CLI | Union |
|---|---:|---:|
| Native nonliteral string-capture head | 0 | 2 |
| Native direct integer status snapshot | 0 | 1 |
| C nonliteral string-capture head | 0 | 2 |
| C direct integer status snapshot | 0 | 1 |

The predicted full sweep is **8837**: native **4521**, C **4316**, totaling
**8824 passing / 13 existing C skips**. The instrumented sweep matches.
All **8789** baseline statuses, saved at rowid **5907352**, are unchanged;
the additional **48** IDs are exactly the new dual-mode capture checks.
Earlier temporary-sentinel sweeps recorded 46 of those IDs before the last
two were added; final reruns introduce zero first-ever IDs. No unexpected
retirements or previous-row flips. The first-vs-current liveness query
remains **260**, proving the history comparison can report differences.

Ordinary release tools were rebuilt with `mk cov-reset`, then both final
gates were repeated: **165 sash + 27 convert + 20 expand** units, and the
same **8837** source checks with every historical assertion above unchanged.
Both global ID-collision checks are empty. Independent source-plan totals
are exactly 4521/4316; the only plan/token discrepancy is coverage.sh's
known three-execution variable-ID loop.

Formatting, clippy with tests, totality, whitespace and staged touched-function
complexity gates pass without a bypass. Shared string-target collection and
pipeline interning keep dispatchers shallow; factoring unconditional write-end
closure preserves the original per-pipe descriptor-close order.

A persistent-directory compiler/program canary produces the expected capture
output and no profiles, both with and without explicit `LLVM_PROFILE_FILE`,
and with the runtime profile directory configured. The canary executable and
empty probe workspace are removed. Source provenance remains current after
restoration. No coupled finding is deferred.
