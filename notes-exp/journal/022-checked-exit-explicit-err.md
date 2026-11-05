# Checked EXIT and explicitly handled ERR

**2026-11-05.** DG directed adoption of implemented native DD128.
The AI migrated the C++ lifecycle, added five fixture groups and the
fifteenth harmless source pair, and extended the future consumer task.
[DD#15](../decisions/DD15-checked-exit-explicit-err.md) supersedes DD#5.
No native implementation, IR reader/emitter, or producer was changed.
The protected producer proposal remains untouched.

## Separate terminal causes, not a renamed END

The synchronous driver borrows optional EXIT and ERR callbacks; `nullptr`
means absence, not substitution. A body can return void for zero completion
or exactly `std::int64_t` for the actual fall-through status. Handler entry
and callback registration allocate nothing.

Normal fall-through, explicit full-integer zero, and cooperative EXIT
signals select EXIT. Explicit nonzero exit and uncaught failures select
ERR. Status remains full i64 until the final process byte. Thus completed
status 1 selects EXIT, but explicit 256 selects ERR even though its parent
sees zero. The only paths are main -> EXIT, main -> ERR, and main -> EXIT
-> ERR. Missing handlers never substitute one another.

EXIT is checked, including called functions, operands, and setup.
Uncaught failure or explicit nonzero exit enters ERR; explicit zero
terminates. Successful handler completion preserves its incoming status.
First fatal arithmetic/heap failure in main or EXIT also attempts ERR.
A second fatal fault in ERR reports and terminates without recursive entry.
Unhandled operational host ERR exceptions still escape, because the
future source contract requires explicit handling rather than an implicit
catch-and-continue layer.

The migrated lifecycle pair uses real process-global collection state and
free handler functions. New
[`terminal_report.cpp`](../../sash-cpp/examples/terminal_report.cpp) and
[its sash counterpart](../../sash-cpp/examples/terminal_report.sash) pin:

```text
main: saved 17
EXIT: incoming 0
ERR: incoming 256
ERR: saved 23, pending initial
```

An EXIT capture calls a real function that completes `saved = 23`, writes
private output, and exits 256. Assignment to `pending` never completes.
ERR sees the actual saved write and original pending string, not a
hypothetical final-state copy. C++ unwinding disposes the interrupted
capture/readers before dispatch; no native capture-stack port was needed.
Invoker channels are restored before each callback, including EXIT -> ERR.
PID guards keep parent handlers out of raw fork children.

DG's source-placement ruling is comprehensive: source function definitions
are directly top-level only, never in either handler or another nested
scope. Default static backing storage precedes entry, but source initializer
expressions belong inside the protected body. Future consumer admission
must preserve ERR's explicit handling and conservative incoming-taint rules;
the prototype does not add a temporary source validator.

## Bootstrap readiness is independent of snapshots

Startup heap dispatch exposed an ownership requirement: source reservations
cannot be a member of the partially constructed invoker snapshot object.
The driver now borrows and validates initializer-list metadata without
allocation, then constructs snapshots in a separate optional owner.
The reservation scope survives snapshot failure and both callbacks.
Duplicate/unsorted entries need no sorting or owned heap representation.

The calibrated fixture first warms ordinary exception metadata, then
limits descriptors to three and denies allocation. A backup failure's
`SetupError` construction becomes `bad_alloc`. ERR starts with no body
execution and exactly one rejected allocation; entry has not allocated.
The callback explicitly recovers resources, fills lower descriptor slots,
and proves a capture cannot occupy the still-reserved source descriptor.
Removing startup dispatch, reservation lifetime, or membership each fails
its own compiled control.

Ordinary channel setup failure still runs neither body nor handler.
Failed invoker restoration prevents unsafe callback entry; broken cleanup
and signal-handoff guarantees retain the prior nonallocating fatal policy.
Invalid host metadata is not admitted source and has no startup callback.

## Signal receipt versus compiler-owned dispatch

`checkpoint(bool deferred = false)` mirrors the native compiler's
protected-window decision. True defers EXIT dispatch, not `signum()`.
Ordinary failure and explicit exit keep their own routes, and either
terminal phase prevents redispatch.

Fixtures cover acquisition/attest and direct failed-branch replacement,
single-use returns, reader conditions and false/completion edges, and
failure/exit precedence while a signal is pending. No helper gains a
hidden checkpoint or runtime taint heuristic.

The [future emitter task](../tasks/ir-signal-checkpoint-emission.md) now
covers these protected placements as well as DG's requested conditional
presence. Actual generated calls, not strings/comments, must reject both
unconditional insertion and complete omission, ignored deferral, and
missing return/reader polls. `run`'s fixed entry/completion checks are
outside that test. There is still no consumer to run it against.

## Executed evidence

Predictions were five new groups (**68 to 73**), one new golden
(**fourteen to fifteen**), and three project files (**64 to 67**).
All matched. The frozen project-input manifest was verified throughout
the compiled reversions and real-host validation.

| Gate | Result |
|---|---|
| Ordinary GCC and Clang | 73 groups and fifteen C++ goldens each |
| Supported GNU Linux ASan | 73 groups and fifteen C++ goldens |
| Actual Apple Clang/libc++ and Apple ASan | 73 groups and fifteen C++ goldens each |
| Existing Linux fault gates | Passed on both ordinary compilers |
| Redirect fault controls | Fourteen calibrated controls on both ordinary compilers |
| Existing error catalog | All 169 cases on both ordinary compilers |
| GNU all-category UBSan | Lifecycle selector and both lifecycle goldens |
| Independent detection canaries | GNU ASan/UBSan and actual Apple ASan rejected genuine faults |
| Compiled reversions | All seventeen produced logical fixture failures, not crashes |
| Pinned actual-CLI pairs | All fifteen matched independent goldens |
| Formatting | Existing formatter gate passed |
| Actual Mac input parity | All 67 project files matched |

This is a focused UBSan result, not a claim that the full runtime,
combined-sanitizer gate, or original zero-fd conflict has been resolved.
The [tooling task](../tasks/sanitizer-host-compatibility.md) stays open.
The existing pre-limit scaffolding now also warms `bad_alloc` metadata,
without disabling categories or removing real resource failures.

The seventeen compiled reversions test cause-based routing, full-width
explicit and fall-through status, both missing-handler rules, both
re-entry guards, arithmetic/heap dispatch, allocation-free ERR entry,
signal deferral and handler signal guards, startup ERR, startup reservation
lifetime, the second invoker restoration, child PID guards, and source-fd
membership. The runner compiles real mutated lifecycle/POSIX objects and
uses the final fixture object and helper. Its no-op negative control
survives and makes the runner fail, proving rejection is not automatic.

The ineffective-injection control runs an isolated copy of the real fault
driver with `PYTHONOPTIMIZE=1`. A valid, deliberately unreached ordinal
leaves no injection record; the channels assertion rejects its status zero.
An initial ordinal 999999 was rejected by strace itself and did not count
as evidence. The corrected control fails for the intended assertion.
Apple's use-after-free canary reports the precise ASan diagnostic and
aborts with shell status 134; its first wrapper incorrectly required GNU's
status 1. The corrected wrapper verifies the real platform boundary.
Core dumps were disabled for that control.

An independent totals check initially included two ordinary redirect groups
in the fault count. Restricting it to the actual `fault`/`retry` records
confirms fourteen controls in both final logs; the main group count is 73.
These were control-accounting corrections, not production-code changes.

## Reference and remaining work

The actual native compiler/runtime pins came from implemented DD128 at
`09a5bd497cae238b7412ea8357943e541b590c1d`:

```text
compiler: 4430114af36d1e3d4772ec3afb84a2f7aba78565c638d15d6e690b8e53653c58
runtime:  6a41b497e7742dca617dcce7a0b63eff6d383b853b5ca11a7f861ef1bbdb5de3
```

The matching archive was supplied explicitly. Independent harmless probes
confirmed nonzero fall-through versus full-width explicit exit routing.
Of the old fourteen pairs, only the former END pair needed migration;
the current fifteen all pass without changing their independent expected
behavior except the intentional END-to-EXIT label.

Native schema 2 exposes optional `:exit-block` and `:err-block`, not
`:end-block`; an empty handler differs from absence. Future consumer work
includes those fields, top-level definition placement, explicit ERR
admission/taint, global initialization, and protected poll planning.
No IR work was started.

Persistent redirects remain the next runtime unit. Public wait,
forked-C++/background ownership, remaining formatting, and the separately
tracked sanitizer/tooling work remain on the
[roadmap](../tasks/cpp-transpiler-runtime-before-ir.md).
