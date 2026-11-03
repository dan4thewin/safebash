# Temporary redirections and compiler-supplied FD metadata

**2026-11-03.** DG authorized the trial after clarifying that descriptor
selection is a compiler/runtime seam, not another language-policy fork.
The AI implemented temporary redirects, source-fd reservations, saved
invoker channels, six fixture groups, and the fourteenth harmless
[C++](../../sash-cpp/examples/file_report.cpp)/
[sash](../../sash-cpp/examples/file_report.sash) report.
[DD#14](../decisions/DD14-temporary-redirections.md) records the contract.
There is still no IR reader, emitter, or producer implementation.

## Metadata rather than magic private numbers

The inspected native redirect IR already contains `:fd`, `:op`, and
`:target`. `program_referenced_fds` collects redirect endpoints, numeric
duplication sources, and close operands across functions, captures, and END.
The compiler supplies that set to the native runtime. Private copies are
runtime/OS allocations; the native minimum-100 floor is not a fixed channel
address.

This target accepts the source-fd set through `run`, reserving 0/1/2
implicitly. Private pipe, capture, backup, and diagnostic placement skips
every declared number, even when closed. A sorted owned set supports active
scopes, and an allocation-free intrusive owner registry rejects a late set
that would overwrite a live private descriptor. The future producer should
export its computed set rather than requiring duplicate consumer analysis.
The protected producer proposal was not edited.

`Redirect` factories own paths and express input/output/append/read-write
or destination-first duplication. A void `redirected` callback applies the
specifications left to right. Metadata/NUL validation and all snapshots
precede file effects; setup failure skips the body and restores bindings,
not earlier truncation. Normal and exceptional completion preserve the
previous closed state and fd flags. The shared restore helper also serves
in-process capture.

`run` snapshots invoker stdio after reserving source fds. Runtime errors and
fatal cleanup use saved invoker stderr even inside command-file redirects;
closed invoker stderr has no fallback. Original channels are restored
before END. Failed channel setup runs neither body nor END; failed invoker
restoration prevents END. Persistent exec/rebinding remains separate.

## Cleanup regression, then an assertion that could not see cleanup

The broader GCC catalog exposed `process-pipe-relocate-writer` exiting 139
after its expected EMFILE diagnostic. Its trace showed the first relocated
endpoint, fd 4, never closing when relocation of the second endpoint failed.
A real fd-limit reproduction under GDB stopped in the intrusive registry's
destructor traversal with a stale owner address.

Fallible initialization of the returned Pipe aggregate had lost the earlier
owner on this GCC toolchain. Named reader/writer locals now complete both
fallible relocations before an aggregate of noexcept moves is returned.
A second real-limit control exposed the same problem in the braced invoker
snapshot array: earlier backups survived failure of the third copy.
Three directly initialized members now guarantee their cleanup.
The controls assert both released kernel fds and successful subsequent
reservation, not merely status 2. Both relevant catalog cases then passed.

The first mutation census killed nine of ten candidates. Removing
`RedirectFrame`'s destructor restoration still passed: the caught-body
fixture checked only the original integer exception and written prefix,
while the main boundary repaired the descriptors later.
The replacement fixture nests input/stderr scopes inside a capture,
throws after rebinding stdin/stdout/stderr and fd 9, then immediately
reads/writes through the restored outer channels and checks fd 9 is closed.
The prefix, stderr bytes, and extra file effects survive. This pins
restoration before the main boundary; the same mutation now fails.

## Source-shaped report and admission repairs

Real `write_records`, `write_merged`, `write_unmerged`, `write_reserved`,
`read_records`, and `collect_files` functions retain the source structure.
The entry declares fd 3. The report demonstrates truncation/append/readback,
a nested redirected function, both duplication orders, an inherited source
descriptor, and a caught missing-input failure with an empty command stderr
file. The independent eleven-line golden includes restored stdout.
The harness uses a private directory and cleans six exact files.

The first native trial rejected a redirected compound block with SB4021.
Both sides now redirect named function commands instead. Native DD#39
requires an escaped literal hyphen in bracket expressions, so equivalent
patterns escape their literal hyphens without narrowing their domains.
The top-level input was renamed `workdir` to avoid DD#86/SB5049
local/global shadowing.
All fourteen pinned real-CLI pairs then passed; no native admission changed.

## Executed evidence

Predictions were six new groups (**62 to 68**), one new golden
(**thirteen to fourteen**), and five project files (**59 to 64**).
All predictions matched.

| Gate | Result |
|---|---|
| GCC and Clang | 68 groups and fourteen C++ goldens each |
| Supported Linux ASan | 68 groups and fourteen C++ goldens |
| Real Apple Clang/libc++ and Apple ASan | 68 groups and fourteen C++ goldens each |
| Existing Linux fault gates | Passed on both ordinary compilers |
| New redirection fault driver | Fourteen calibrated controls passed |
| Existing error catalog | All 169 cases passed on both ordinary compilers |
| Redirection-only complete-category UBSan | Six-group selector and file golden passed |
| Independent detection controls | Linux UBSan/ASan and Apple ASan rejected genuine faults |
| Compiled mutations | All ten rejected through fixture failures |
| Pinned actual-CLI pairs | All fourteen matched independent goldens |

The new child-free fault baseline calibrates actual syscall ordinals and
checks injected errors, body/END reachability, command-file bytes, and
restored output. Ten setup/completion faults, fatal unwind restore, and
three EINTR recoveries run. A wrapper removing injection makes the gate
fail even with `PYTHONOPTIMIZE=1`; the driver ignores Python optimization
settings rather than permitting disabled assertions.

The current invoker copies also moved catalog diagnostics away from fixed
fd 2. Five diagnostic selectors now calibrate the runtime's actual
descriptor by trace evidence; the catalog count and expected bytes/statuses
are unchanged. The Clang fixture loop borrows its Redirect values instead
of triggering its copy warning under Werror.

The Mac's published 59-file baseline was verified before transfer.
Identical 64-file sources passed ordinary and ASan gates. The strengthened
fixtures were synchronized and rerun on both Mac builds, with 128 matching
before/after hash lines. Final project-document synchronization changes
only README/SOP/tour text, not the tested code. Protected-draft and pinned
input checks stayed green.

## Sanitizer instrumentation needs initialization, not fewer categories

The first focused GCC UBSan probe timed out in the real second-endpoint
allocation failure. Its trace reached EMFILE with only low fd 0 free.
A debugger-owned reproduction stopped in sanitizer
`ReserveStandardFds`, repeatedly duplicating invalid negative fd -9:
its lazy mapping read could open fd 0 but could not raise that fd.

A standalone derived `std::system_error` program with no sash dependency
reproduces the cold timeout (**124**). The same real limit/failure after
ordinary exception initialization succeeds (**0**), as does the unconstrained
control. Fd-limit scaffolding now initializes ordinary exception/vptr paths
with a harmless, rejected NUL executable before closing slots or lowering
limits. Both targeted allocation failures remain real, and every UBSan
category remains enabled. The independent signed-overflow canary still
returns 1 with the intended diagnostic. This is redirection-only UBSan,
not a full-runtime or zero-fd-backlog claim.

| Compiled mutation | Rejected contract |
|---|---|
| Ignore allocation reservations | Private/source fd separation |
| Retain reserved raw opens | Private placement |
| Admit missing metadata | Preflight before effects |
| Reverse application order | Left-to-right duplication |
| Skip unwind restoration | Outer channels restored before main |
| Skip normal restoration | Nested channel restoration |
| Report through current fd 2 | Invoker diagnostics |
| Drop original fd flags | Original close-on-exec state |
| Restore fallible returned Pipe aggregate | Partial endpoint ownership |
| Restore fallible snapshot array | Partial channel ownership |

No compiler/link failure counted as a killed mutation. Healthy production
controls passed before and after the mutation census.

Reference inputs retain these hashes:

- compiler: `5fd4fbcdf0ab5ef76e3cd5cd86bca1eed77559b4c6295867623b5a77e2a5f04d`
- archive: `d26debdce3f691f0880d5a2745bdf2e1f7c875815c25a8e4fc676bba48f10dfa`

## Scope

This completes temporary file/FD redirects and invoker-channel restoration,
not persistent exec redirects, public wait, forked-C++/background ownership,
remaining formatting, or IR consumption. Existing signal/checkpoint and
explicit END policies remain unchanged. Experiment records remain under
`notes-exp/`; no native code/notes changed. The protected proposal remains
untracked and excluded from transfer and publication.
