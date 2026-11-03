---
title: "Build the external C++ transpiler runtime before the IR changes"
status: open
kind: feature
depends: []
gates:
  - "a standalone C++17 library supports source-shaped hand-written programs without compiler internals or an IR reader"
  - "Linux process launch, redirection, memfd capture, streaming pipelines, and shutdown work together"
  - "sparse arrays and maps use std::map; regex operations use std::regex with ECMAScript grammar"
  - "resource ownership and failure paths are exercised through native executable fixtures"
  - "shared POSIX ownership and small host adapters support real Linux and macOS builds"
  - "the remaining consumer work is source-to-C++ emission and call-site semantics, not unfinished runtime machinery"
---

## Purpose and relationship to the producer task

Build the runtime for the separate, readable C++ transpiler now, without
waiting for changes to `sash --emit-ir`. Exercise it with hand-written C++
programs shaped like the intended output: ordinary functions, variables,
references, loops, branches, and standard containers, with small runtime
helpers for operations C++ does not directly provide.

The sibling draft, `emit-ir-json-for-c-transpiler.md` (kept uncommitted as
requested), prepares the producer. These tasks are independent: neither
is a prerequisite for the other. This task does not add an IR reader,
change the compiler, or implement a temporary interpreter.

Implementation lives under `sash-cpp/`, not inside `sash-rt` or the existing
SSA-based C backend. Its journal, tasks, decisions, and status live under
`notes-exp/`, separate from the native compiler's `notes/`.

The front end is undergoing active fixes. Runtime development must not
freeze its current accepted forms, reproduce its bugs, or depend on a
particular arrangement of its passes.

## First runnable milestone - implemented

`sash-cpp/examples/greetings.cpp` is a hand-written equivalent of the
adjacent sash program, retaining a function, sparse map, loop, regex
condition, and capture. It prints `Hello, Ada!` using the standalone
library. Build and run it with `cd sash-cpp && mk -p 1 test`.

The first slice provides command outcomes, checked wrappers, a specific
exception boundary, raw byte output, ECMAScript construction, and nested
memfd capture with descriptor restoration. GCC, Clang, and host-GCC ASan
exercise eight native fixture groups and the example's expected output.
The [journal](../journal/001-first-cpp-program.md) records the scope and
the [sanitizer task](sanitizer-host-compatibility.md) records the remaining
UBSan host limitation.

This was not completion of the roadmap. Pipeline/reader ownership and
lifecycle milestones are recorded below; general redirection and the
collection-operation library remain. The sash counterpart compiles and
runs with DG's supplied `~/src/safebash/_tmp/sash` and the runtime archive
from the same checkout. Both executables produce exactly `Hello, Ada!\n`,
also matching the independently specified output. The already-filed local
rustix build failure does not block this reference run.

## Second runnable milestone - implemented

`job_report.cpp` and its sash counterpart retain a real worker-label
function, sparse records, a two-round while loop, for traversal, and a
regex branch. The function runs an external printf inside capture;
external test commands supply a condition and explicitly discarded
results. Both programs match their independent eleven-line expected output.

DG selected single-threaded fork/exec after comparison with a posix_spawn
hybrid; [DD#2](../decisions/DD02-foreground-fork-exec.md) records the choice.
The library now has foreground `try_execute`/`execute`, distinct normal
exit/signal/setup results, prebuilt argv, inherited environment, and child
ownership through wait. There is no shell fallback or PATH search.

Fourteen native fixture groups pass with GCC, Clang, and host-GCC ASan.
Linux strace probes exercise fork failure, interrupted wait, partial
descriptor relocation, and setup reporting without child writes.
The [second journal entry](../journal/002-job-report-and-foreground-processes.md)
records the reporting-write bug found and fixed during implementation.

This completed the first Linux launch/capture slice; the subsequent
[macOS milestone](../journal/008-macos-posix-runtime.md) is now implemented.
It does not complete
background/public wait, forked C++ stages, call-site result handling,
pipelines, redirects, or the full lifecycle.

## Error-path demonstration - implemented

`cd sash-cpp && mk -p 1 error-demo` now runs 112 isolated cases: 53 basic
outcome/exception cases, 45 injected faults, and 14 EINTR recoveries.
`examples/error_paths.cpp` exposes the existing checked, caught, and
discarded handling, with unchanged capture targets after failure.
The shell catalog adds setup, descriptor, capture, launch, wait,
diagnostic-write, and fatal cleanup paths.

GCC and Clang run the full catalog. Faults are checked in their syscall
traces, and ordinary/fatal negative controls fail without injection.
A Linux subreaper owns descendants of traced cases, including the fixture
whose runtime owner deliberately cannot reap during fatal cleanup.
The allocation-boundary demonstration throws `std::bad_alloc` explicitly
rather than exhausting the shared host; corrupted internal status is not
manufactured. The basic subset does not need strace.

This is a demonstration of the implemented slice, not completion of later
runtime machinery or per-stage result suppression.
[Journal 003](../journal/003-error-path-showcase.md) records the evidence
and limits of the original 103 cases; journal 004 adds the reader cases.
The runnable catalog is documented in `sash-cpp/README.md`.

## Third runnable milestone - implemented

`stream_report.cpp` and its sash counterpart stream four external records
through a parent-side `while readline`. A real function fills the caller's
sparse collection (`=workers` in sash, an ordinary C++ reference), captures
external worker labels inside the loop, and retains entries at indices
1, 2, and 4 afterward. Both match an independent nine-line expected file.

`read_from` checks one producer's completion; `try_read_from` retains its
typed outcome. The borrowed `LineReader` is explicit, never an ambient fd 0
binding. Reader effects precede producer status, and cleanup closes the
pipe before waiting on normal, early, exceptional, and partial-setup paths.
Consumer exceptions remain distinct from producer outcomes. Line records
follow the DD#99/DD#122 reference rules, preserving binary bytes and an
unterminated final record while clearing the target at EOF.

Nineteen native fixture groups and all three C++ goldens pass with GCC,
Clang, and host-GCC ASan. A two-way handshake proves the consumer runs before
producer completion; other cases exercise nested readers/captures, original
stdin, private descriptor inheritance, early SIGPIPE, and ownership failures.
The full 112-case error catalog passes on both compilers, and five new
negative controls reject the same runs with injection removed.
[Journal 004](../journal/004-streaming-worker-report.md) records the scope.

This is the single-producer parent-reader slice, not multi-stage pipelines,
forked C++ stages, public wait, pipeline suppression, general redirection,
or shutdown. Those remain below; the macOS storage choice remains open.

## Fourth runnable milestone - implemented

`pipeline_report.cpp` and its sash counterpart feed printf through
C-locale sort and uniq before a parent reader fills the caller's collection.
The ten-line report retains real functions, while/for loops, captured
labels, and an upstream-failure condition whose final stage succeeds.

DG selected all-stage outcome retention, failure on any stage, and the
rightmost failed stage for checked numeric status
([DD#3](../decisions/DD03-external-pipeline-outcomes.md)).
`read_pipeline` and `try_read_pipeline` reuse launch and line-reader
machinery. Unstarted stages are explicit; setup/cleanup errors are kept
separately from known child exits/signals. Partial construction and
consumer unwinding close all remaining endpoints before waiting.

Twenty-three fixture groups and all four C++ goldens pass with GCC, Clang,
and host-GCC ASan; the supplied native compiler matches all four sash
goldens. The full showcase has 151 cases: 73 basic, 61 faults, and 17
recoveries, passing on both ordinary compilers. Eight new uninjected
controls, two focused selectors, and three implementation reversions
pin the completion and ownership rules.
[Journal 005](../journal/005-external-pipeline-report.md) records the
counter-prelude harness fix and outcome-preservation correction.

This completes multiple external stages with a terminal parent reader,
not forked C++ stages, per-stage result suppression, public wait, general
redirects, or shutdown.

## Fifth runnable milestone - implemented

`capture_report.cpp` and its repaired sash counterpart capture a five-stage
external pipeline in a summary function, nest it inside a framed function
capture, and retain two rounds for later traversal. Both produce the
independent eight-line report. The native capture/status/attest sequencing
was repaired separately in [journal 006](../journal/006-capture-report-repair.md);
this milestone completes the C++ companion and runtime.

`try_capture_pipeline` returns exact counted bytes and a separate pipeline
result; `capture_pipeline` checks completion before returning the string.
The raw consumer shares command preparation, launch, endpoint ownership,
and reaping with the line-reader pipeline. Failed producer output and bytes
preceding a completion error remain inspectable. Read/allocation exceptions
escape rather than becoming EOF or a child verdict. External-only capture
uses a concurrently drained pipe; general in-process capture retains memfd.

Twenty-seven fixture groups and all five C++ goldens pass with GCC, Clang,
and supported host-GCC ASan. The supplied native compiler matches all five
sash goldens. The predicted and observed error catalog is 169 cases:
85 basic, 65 faults, and 19 recoveries. First/second read probes use a live
finite binary producer and baseline-calibrated data fds. A real collector
allocation fault checks descriptor/reap ownership without host exhaustion.
Six uninjected controls and three source mutations pin injection, raw
newlines, failure-byte retention, and independence from memfd.
[Journal 007](../journal/007-raw-pipeline-capture.md) records the scope.

This completes raw external-pipeline capture, not source-level newline
treatment, background-writer policy, call-site result handling, forked C++
stages, redirects, public wait, or shutdown. The runtime roadmap remains
open; no producer change or IR reader is included.

## macOS bring-up - implemented

DG selected an existing Mac for the next milestone. The shared C++17 code
now runs with Apple Clang/libc++ on Apple Silicon. POSIX launch, byte I/O,
restoration, and child ownership are shared rather than duplicated.
Linux retains memfd/pipe2; Darwin uses immediately unlinked mkstemp capture
files, owned pipe/CLOEXEC setup, and `_NSGetEnviron`.
[DD#4](../decisions/DD04-macos-posix-adapter.md) records the contract.

Twenty-eight fixture groups and five C++ goldens pass on both hosts,
including supported Linux and Apple ASan builds. Invalid TMPDIR refuses
Darwin in-process capture before the body, but neither Linux memfd nor
raw pipe capture depends on it. Four isolated Darwin call-error controls
pin setup cleanup; skipped unlink/CLOEXEC mutations turn the relevant
checks red. [Journal 008](../journal/008-macos-posix-runtime.md) records
the real-host evidence. The
[Darwin fault runner](cpp-darwin-fault-harness.md) is follow-up automation.

The pairs now use utility paths common to both hosts and explicit paste
stdin. The native Linux reports remain unchanged. Linux-only strace and
subreaper targets reject Darwin explicitly. No lifecycle or language
policy is introduced; the full runtime task remains open.

## Sixth runnable milestone - lifecycle implemented

`lifecycle_report.cpp` and its sash counterpart retain collection state
from a real captured function, discard only another function's returned
int, and explicitly exit into END. END catches one failed command,
discards another, and traverses the saved collection; all source functions
are defined outside END. Both match the independent eight-line golden.

The driver distinguishes normal completion, explicit exit, known uncaught
body failure, and a signal request. Successful body unwinding closes
pipeline endpoints before reaping and restores capture before owner-only
END. An explicit END exit overrides status without re-entry. There are
no automatic END failure guards or inherited unchecked function mode.
Invalid unhandled C++ END exceptions escape; mandatory source handling
and the definition restriction remain future front-end enforcement.

Shared POSIX signal handlers only store lock-free most-recent and pending
EXIT integers. Checkpoints are explicit at caller/function/loop boundaries,
not hidden in command or reader helpers. Reads/waits retry EINTR, and
blocking work can delay shutdown; no force-kill or runtime timeout was added.

All 35 fixture groups and six C++ goldens pass with GCC, Clang, supported
Linux ASan, actual Apple Clang/libc++, and Apple ASan. The real native
compiler matches six goldens, and the Linux catalog remains 169 cases.
Six new signal-call probes reject missing injections; two deny allocation
while forcing private handoff failures. Pre-acknowledgement consumer failure
closes the fixture's exec-private writer before reaping. Three isolated
mutations pin separate FLAG/EXIT identity, no implicit END policy, and
main-only END. See [journal 011](../journal/011-lifecycle-runtime.md).
This milestone does not implement forked C++ stage ownership or the
remaining value, argument, redirect, and wait operations.

## Seventh runnable milestone - collections and arguments implemented

`option_report.cpp` and its sash counterpart use real preparation and
argument-reporting functions. A collection is forwarded to a preparation
helper; sparse writes/deletion, population length, typed-zero
reads, joins, pairwise map construction, copied keys, and argument
dispositions produce an independent seventeen-line golden.

The runtime keeps standard-map aliases visible. Shared live range adapters
borrow the collection, save a copied key, and resume at its next greater
present key after each body. They return copied loop values, safely
handling current-cell deletion, clearing, later insertion, and mutation
through aliases. This implements DD#7's sorted live walk, not a claim that
the native table cursor has identical mutation behavior.

Defaults and requirements share scalar/cell rules and evaluate operands
lazily. Joins and argument snapshots preserve counted bytes, stored empty
cells, and typed integer zeros. Only explicit elision removes a scalar
identity; an empty splat default produces one argument. Prepared argument
vectors are owned before launch and still use the existing NUL rejection.

All 41 groups and seven C++ goldens pass with GCC, Clang, supported Linux
ASan, real Apple Clang/libc++, and Apple ASan. The supplied native CLI
matches seven sash goldens; both Linux compilers retain the fault gates
and 169-case catalog. Five isolated implementation mutations fail for the
intended assertions, not timeouts. See
[journal 014](../journal/014-collection-argument-runtime.md).

The current pair forwards one slot to conform to DG's 2026-11-02 DD#100
amendment in the active reference checkout, rather than passing the same
collection twice. Its golden is unchanged. The library retains ordinary
C++ references and generic alias-safety fixtures; the producer owns source
call admission. Journal 014 records the earlier duplicate-slot demonstration.

The IR reader, emitter, producer, native compiler, argv/environment ingress,
and remaining value/read, redirect, and wait families are unchanged.

## Eighth runnable milestone - checked arithmetic implemented

`arithmetic_report.cpp` and its sash counterpart retain a real int-returning
subtotal function, counting for, while, if, and a lazy ternary. Both match
an independent ten-line report. Integers remain `std::int64_t`; checked
functions implement native DD#25 overflow, division/remainder, power, and
unbounded nonnegative shift semantics.

[DD#8](../decisions/DD08-checked-integer-arithmetic.md) records DG's choice
of functions rather than a numeric wrapper. The AI implemented standard
bounds checks, unsigned-bit shifts, and checked exponentiation by squaring.
`ArithmeticError` is distinct from command verdicts, reports status 2 at
the ordinary body boundary, and receives no automatic END handling.

All 44 groups and eight C++ goldens pass on GCC, Clang, supported Linux
ASan, actual Apple Clang/libc++, and Apple ASan. The pinned supplied native
CLI/archive match eight sash goldens; ordinary Linux compilers retain the
fault gates and 169-case catalog. Arithmetic-only UBSan passes with two
independent detection canaries, and six isolated mutations fail their
intended assertions. See
[journal 015](../journal/015-checked-integer-arithmetic.md).

This completes checked operations, not accountable string-to-int
conversion, numeric ingress, or the remaining text/read families. No IR
reader, emitter, producer, or native code changed.

## Ninth runnable milestone - numeric inputs implemented

`numeric_report.cpp` and its sash counterpart use explicit program inputs,
owned argv snapshots, optional getenv, shared strict conversion, and a real
charge function feeding checked multiplication. With `08 0` and a hex rate,
both match an independent eight-line report.

[DD#9](../decisions/DD09-numeric-ingress-and-conversion.md) records the
source-authoritative input/conversion contracts and the AI's reversible
implementation choices. Missing getenv is a result, not an automatic abort;
set-empty remains present. Invalid and out-of-range numeric text follows
default/required disposition, valid zero stays zero, and scalar/string-cell
conversion shares an allocation-free parser and lazy requirement renderer.
Actual metadata/allocation failures remain visible; required conversion
preserves the destination and existing body/END boundaries.

This completes the bounded argv/environment and strict numeric unit, not
the remaining text/read/regex families or IR consumption. No native code
or producer changed.

## Tenth runnable milestone - byte-string operations implemented

`string_report.cpp` and its sash counterpart retain a real option function
and string-cell traversal. `COUNT=08` is sliced into key/separator/value:
only the key becomes `count`, while unchanged `08` converts strictly to 8.
Both match an independent seven-line report; there is no implicit trimming,
Unicode normalization, or malformed-number repair.

[DD#10](../decisions/DD10-counted-byte-string-operations.md) records shared
value/cell operations: checked byte length, owned signed/clamped slices,
ASCII-only upper/lowercase copies, and nonempty print/graph predicates.
Omitted and explicit-zero length differ; signed minima, interior NUL and
high bytes remain defined. Allocation failure escapes instead of publishing
an empty replacement. Pure helpers add no hidden checkpoints.

This completes basic byte-string operations, not regex replacement, bulk
reads/splitting, remaining formatting, or IR consumption. No native code
or producer changed. All 51 groups and ten goldens pass across GCC, Clang,
Linux ASan, real Apple Clang/libc++, and Apple ASan. Text-only UBSan,
eight isolated negative controls, ten real-CLI source pairs, and the
unchanged 169-case catalog are recorded in
[journal 017](../journal/017-counted-byte-string-operations.md).

## Eleventh runnable milestone - regex matching and extraction implemented

`regex_report.cpp` and its sash counterpart retain a real function,
whole-match extraction, capture arrays, repeated group extraction,
pairwise map construction, and numeric conversion. The harmless
`COUNT=08;EMPTY=;RATE=0x7` input matches an independent nine-line report.

[DD#11](../decisions/DD11-regex-matching-and-extraction.md) separates DG's
pragmatic engine/iteration choices from their observable target policies.
Subjects remain fully counted through NUL, and standard regex_iterator
preserves original anchor context and retries nonempty matches after empty
ones. Native's retained first-NUL boundary and suffix progression are not
silently copied or described as bugs.

Output capture arrays are built locally and swapped on completion: no-match
clears them, while exceptions preserve the previous target. This simple
C++ strategy also supports a subject borrowed from its output target.
Literal NUL patterns now fail explicitly with guidance to use `\x00`,
after a direct standard-library probe found incorrect matching. DG requested
a [native backlog item](../../notes/tasks/reject-literal-nul-patterns-and-source.md)
separately; no native code changed.

This completes matching/extraction, not replacement templates, bulk
reads/splitting, remaining formatting, or IR consumption.
The fresh reference compiler's DD#127 checks also required adjacent
source attests in the job/reader/lifecycle pairs and an internal attest
in the shared summary producer. Their golden bytes remain unchanged.
All 55 groups and eleven goldens pass across GCC, Clang, Linux ASan,
real Apple Clang/libc++, and Apple ASan. Regex-only UBSan, nine isolated
mutations, eleven pinned real-CLI pairs, and the unchanged 169-case catalog
are recorded in
[journal 018](../journal/018-regex-matching-and-extraction.md).

## Twelfth runnable milestone - regex replacement implemented

`rewrite_report.cpp` and its sash counterpart retain a real function,
first/global rewrites, a condition, literal dollar/backslash output, and
string-cell traversal. The harmless `COUNT=08;RATE=7` input matches an
independent ten-line golden with equivalent C++/native templates.

[DD#12](../decisions/DD12-regex-replacement-templates.md) records DG's
selection of standard C++ notation. Subjects/formats are counted through
NUL/high bytes; normal misses return owned unchanged input, and exceptions
preserve caller assignment. Standard iteration and explicit-range match
formatting avoid the measured convenience-overload format truncation.
No bespoke parser, runtime adapter, or hidden checkpoint was introduced.

All 58 groups and twelve goldens pass across GCC, Clang, Linux ASan,
real Apple Clang/libc++, and Apple ASan. Replacement-only UBSan, eight
compiled mutations, twelve pinned real-CLI pairs, independent extra inputs,
and the unchanged 169-case catalog are recorded in
[journal 019](../journal/019-regex-replacement-templates.md).

This completes replacement, not bulk reads/splitting, remaining formatting,
or IR consumption. Native-template translation remains deferred to the
future consumer; no native implementation or producer changed.

## Thirteenth runnable milestone - NUL record reads implemented

`record_report.cpp` and its sash counterpart retain a real function,
caller-owned array, and ordinary field traversal. The default
`Ada||Grace|Linus|` input is converted upstream to NUL, preserving a real
empty field. External NUL records replace a stale cell, are joined, and
are cleared with empty input. Both match an independent ten-line golden.

[DD#13](../decisions/DD13-nul-record-reads.md) retains DD#99's NUL-only
terminator semantics: owned dense string arrays, no synthetic final field,
and replacement at empty input/EOF. Counted strings, string-cell values,
borrowed fds, and buffered command/pipeline readers share one splitter.
The simple collector temporarily holds the full input and owned fields;
an existing reader consumes its prefetched tail first.

I/O/allocation failures preserve caller assignment without rewinding
consumed bytes. Borrowed fds remain open and unchanged. Existing owners
close before waiting during unwinding; producer failure after completed
assignment does not roll it back. No flags or hidden checkpoints were added.

All 62 groups and thirteen goldens pass across GCC, Clang, Linux ASan,
real Apple Clang/libc++, and Apple ASan. NUL-only UBSan, ten compiled
mutations, thirteen pinned real-CLI pairs, four independent extra inputs,
59-file Mac parity, and the unchanged 169-case catalog are recorded in
[journal 020](../journal/020-nul-record-reads.md).

This completes bulk reads/splitting, not remaining formatting, general
redirects, public wait, forked-C++/background ownership, or IR consumption.
The actual CLI's current hex-NUL source spelling is used without changing
the native implementation or the protected producer proposal.

## Fourteenth runnable milestone - temporary redirections implemented

`file_report.cpp` and its sash counterpart retain real writing/collection
functions, ordered/nested redirects, both duplication orders, and explicit
fd 3. The private-directory harness matches an independent eleven-line
golden and leaves a caught failure's stderr file empty.

[DD#14](../decisions/DD14-temporary-redirections.md) records DG's compiler/
runtime seam: source descriptor numbers are metadata; private pipe, capture,
backup, and diagnostic copies are runtime/OS allocations. `run` accepts the
source-fd set before allocation, with 0/1/2 implicit. The future producer
should export its already-computed set, without encoding private numbers
or requiring duplicate consumer analysis. No producer changed.

Temporary input/output/append/read-write/duplication applies left to right.
All snapshots precede effects; partial failure skips the body, restores
bindings/flags/closed state, and releases earlier allocations without
undoing truncation. Runtime diagnostics use invoker stderr; invoker stdio
is restored before END. Persistent exec/rebinding remains pending.

All 68 groups and fourteen goldens pass across GCC, Clang, Linux ASan,
real Apple Clang/libc++, and Apple ASan. Fourteen new fault controls,
redirection-only complete-category UBSan/canaries, ten compiled mutations,
fourteen pinned real-CLI pairs, 64-file Mac parity, and the unchanged
169-case catalog are recorded in
[journal 021](../journal/021-temporary-redirections.md).

This completes temporary redirections, not persistent exec, public wait,
forked-C++/background ownership, remaining formatting, or IR consumption.
The protected producer proposal remains unchanged.

## Starting points selected by DG

| Area | Starting point |
|---|---|
| Platforms | Linux first; macOS follows soon, rather than becoming an indefinite future port |
| Language | C++17 |
| Launch and threading | Single-threaded fork/exec, selected in DD#2 |
| Sparse collections | `std::map`, with numeric keys for indexed arrays and string keys for maps |
| Integer operations | Ordinary `std::int64_t` values with checked functions and native DD#25 arithmetic/shift rules, selected in DD#8 |
| Regex | `std::regex` with `std::regex_constants::ECMAScript` |
| Regex replacement | Standard C++ templates, selected in DD#12; native-template translation belongs to the future consumer |
| NUL record reads | Owned dense arrays with DD#99 terminator semantics, retained in DD#13; other separators are converted upstream |
| Descriptor seam | Compiler-supplied source-fd set; runtime/OS private allocation; temporary scopes and invoker channels retained in DD#14 |
| General in-process stdout capture | Linux `memfd_create`, not a named temporary file or a synchronously filled pipe |
| Failure propagation | Exceptions for uncaught failures; retain caught outcomes and explicit suppression rather than making every nonzero command exit throw |
| External pipeline completion | Retain every stage; checked pipelines fail on any stage and use the rightmost failure, selected in DD#3 |
| Generated-program shape | Free functions and ordinary C++ control flow, not a bytecode engine or an SSA dispatcher |

These are external-target choices, not changes to native sash. In
particular, standard-map key order is acceptable instead of native
insertion order. Apply the chosen order consistently to keys, values,
joins, splats, and other collection traversals.

The implemented Darwin storage extension does not reopen Linux memfd.
Darwin shared-memory descriptors cannot service ordinary reads/writes,
so the fd-1 capture contract uses an unlinked regular file, not shm_open.

ECMAScript here means the C++ standard library's modified ECMAScript
grammar, not an unspecified JavaScript engine. Do not substitute POSIX
ERE or the Plan 9 engine. Choosing a corresponding Rust-side pattern
checker is separate producer work and does not block this runtime.

Use the normal C++ standard library and host C/POSIX interfaces. Do not
port the native allocator, `_start`, assembly stubs, hash table, or regex
engine, and do not link `libsash_rt.a`. Small static Linux binaries are
not the constraint driving this project.

## Finding relevant responsibilities later

Paths and names below are discovery hints. Locate the current owner by
its responsibility and callers; do not apply changes by historical line
number or copy an obsolete helper.

| Responsibility | Where to start | What to recognize |
|---|---|---|
| Existing runtime operations | `sash-rt/src/lib.rs`, `inner.rs`; runtime section of `docs/GUIDE.md` | OS boundaries, string/number operations, collection operations, captures, and child ownership |
| Error and result semantics | DD#55 and DD#70 | Command verdict versus setup failure, condition handling, suppression, pipeline aggregation |
| Descriptor and diagnostic channels | DD#120; runtime fd save/restore and reservation helpers | Invoker channels distinct from temporary command redirects, private descriptors avoiding source-used descriptors |
| Pipeline reader lifetime | DD#122; pipeline lowering and reader-cleanup responsibilities under `sash/src/transform/` | Parent readers with private fds and pending children; cleanup on control-flow exits |
| General and buffered capture | Runtime capture helpers; `transform/capture_purity.rs` or its successor | In-process capture, nested frames, and the difference between collecting helper writes and redirecting actual fd 1 |
| Shutdown and signals | Experiment DD#5 for END-only handling; native DD#56 for signals/lifecycle | Explicit END handling, signal-safe recording, ordinary-context shutdown, END at most once |
| Explicit wait | DD#126; runtime wait and cached-reader-status helpers | Reaped pid separate from child status, shared ownership when public wait meets pipeline completion |
| Collections, reads, and arguments | DD#79, DD#99, DD#100, DD#123; collection and read helpers | Sparse cells, replacement on read, reference parameters, forwarding, and aliasing |
| Strings and arithmetic | DD#12, DD#25, DD#77, DD#87 | Counted bytes, OS string boundaries, conversions, and defined arithmetic faults |
| Pattern operations and paths | DD#39, DD#63, DD#73; regex, glob, and file-test helpers | Operation families and edge cases, not a requirement to retain the native engines |

The DDs explain the native language and its failure history. Use them as
an inventory of behavior to retain or explicitly replace, not as a blanket
compatibility mandate. A current runtime quirk is not an external-target
requirement merely because a spec records it.

## Function checkedness is settled

DG's 2026-10-29/30 ruling in
[native DD#55](../../notes/decisions/DD55-error-model.md) leaves function
bodies and argument evaluation checked, independently of how the caller
uses the answer. A source `- f` discards only f's returned int or status;
it does not catch failures inside f or nested calls. A condition likewise
consumes the call's own result without unchecking its body.

Keep ordinary C++ calls and handle or discard their returned values at the
call site. Do not add an inherited checkedness parameter or scoped runtime
mode for this. A direct in-process `-` on a void function is SB4076; a
captured int already binds its value, so `$(- int_f)` is SB4076 too.
Stages and jobs have a process status of their own and are separate cases.

The earlier whole-function suppression assumption and its context-passing
fork are withdrawn. Setup-failure suppressibility and residual assignment,
list, and compound `-` forms remain in the native backlog; this update does
not decide them.

## Collection policies for the next unit are settled

DG selected [DD#6](../decisions/DD06-missing-cell-reads.md): an ordinary
missing-cell read returns `""` for strings or `0` for integers without
inserting a key. Stored empty/zero cells remain present. Cell dispositions
retain scalar value semantics, not an unset test. This policy does not
hide failed key evaluation or other operation errors.

[DD#7](../decisions/DD07-collection-indices-length-traversal.md) settles
the remaining collection choices: reject negative array indices, define
length as the number of present cells, and use live sorted traversal.
Live iteration advances to the next greater present key, seeing later
insertions and skipping deletions, including mutation through aliases.
Explicit snapshots and already-prepared argument vectors remain copies.
These choices unblock the next collection/argument unit without settling
the unrelated later operation families below.

## Regex replacement notation is settled

DG selected [DD#12](../decisions/DD12-regex-replacement-templates.md):
standard C++ `$&`/`$1`/`$$` formatting for first/global replacements,
including computed templates. Subjects and formats remain counted byte
values, and DD#11's standard match context/progression applies.
Native-template translation belongs to the future IR consumer, not a
temporary runtime adapter or producer change.

## Choices still requiring discussion

Do not reopen the starting points above. Resolve consequential remaining
forks with DG before building their assumptions throughout the library.

| Choice | Alternatives and tradeoffs | Proposed way to settle it |
|---|---|---|
| Capture completion | Trailing-newline treatment and background writers holding the captured stdout affect which bytes a completed capture yields. | Specify the completion boundary and pin it with direct C++ examples; do not let regular-file EOF accidentally decide the language |

Ordinary command exit statuses remain outcomes even with exception-based
propagation. A false predicate, an integer function
return, a child exit, and a request to leave the entire program must not
collapse into one ambiguous integer.

## 1. Establish the library boundary and native fixtures

- [ ] Define a minimal set of result types for process completion,
  launch/setup errors, captured output, and program shutdown requests.
  Retain normal exit versus signal termination internally; apply any
  source-visible numeric encoding at an explicit boundary.
- [ ] Make ownership explicit for descriptors, children, reader regions,
  redirect scopes, and capture frames. Keep the public interface small;
  do not build a generic resource framework.
- [x] Accept ordinary C++ strings, typed collections, references, and
  callable bodies. Source variables remain normal variables, not entries
  in a runtime name table.
- [ ] Carry a program name and optional source location in diagnostics.
  Hand-written callers can supply them now; richer IR locations are not
  a prerequisite.
- [x] Distinguish operation outcome from its consequence. The eventual
  emitter supplies checked/caught/discarded context; the runtime must not
  infer it from surrounding C++ or conflate setup failure with a command
  verdict.
- [ ] Implement the selected exception approach with command outcomes
  still usable by conditions and explicit discard. Catch specific
  library/OS failures at their boundaries, retaining the
  original error through cleanup. Do not turn them into empty output,
  false success, or a zero value.
- [ ] Add a small native build/run harness and a helper executable that
  can emit exact bytes, inspect descriptors, exit with selected statuses,
  and coordinate signals through handshakes.
- [x] Keep the examples compilable with ordinary C++17 compiler commands.
  No Rust build, compiler pass, IR document, or external test framework
  should be required to exercise the runtime.

The initial examples should already resemble intended output, including
real functions and ordinary early returns. The interface must earn its
readability before the emitter is written.

## 2. Implement descriptor ownership, redirections, and byte I/O

- [x] Isolate Linux-specific operations in a small platform implementation.
  Keep the shared library and future generated sources free of Linux-only
  headers, syscall numbers, and feature-exposure macros.
- [x] Implement owned descriptors, duplication, close-on-exec, complete
  writes, bounded reads, and interrupted-operation handling. Do not
  blindly retry `close` after interruption across platforms.
- [x] Keep short writes, EOF, and read/write errors distinct. Preserve
  embedded NUL and arbitrary byte values.
- [x] Provide the script-used descriptor set as ordinary runtime input.
  Private backups, pipes, diagnostic copies, and capture descriptors must
  avoid those numbers, including numbers initially closed.
- [x] Apply redirections left to right. Cover input, output, append,
  read/write, and descriptor duplication, including nested redirects and
  originally closed targets.
- [x] On partial setup failure, do not run the command; restore descriptor
  state and release successful allocations. Do not claim to undo earlier
  filesystem effects such as truncation.
- [ ] Separate temporary command redirects from persistent `exec`
  redirects. Temporary scopes are implemented; persistent redirects
  remain pending. Successful persistent redirects commit and rebind the
  program's channels rather than being undone by a scope destructor.
- [x] Implement the chosen diagnostic-channel rule, using DD#120 as the
  reference alternative. Restore the appropriate channels before END.
- [x] Choose a disciplined output mechanism. If iostreams and raw
  descriptors coexist, make flushing and fork/redirection boundaries
  explicit so buffered bytes are neither duplicated nor misdirected.

RAII owns resources; it does not by itself settle fallible completion.
Use explicit completion where a failed restore or wait affects program
behavior. Destructors must not throw during unwinding or silently convert
such failures into success.

## 3. Implement process launch and child-status ownership

- [x] Launch explicit executable paths with argument vectors and inherited
  environment data, without a shell or command-string quoting layer.
- [ ] Finish argument construction before the child path. Reject interior
  NUL at executable, argument, environment, and path boundaries rather
  than truncating.
- [x] Distinguish child-creation, fd-setup, and exec failures from a program
  that successfully starts and exits nonzero. A close-on-exec completion
  pipe plus shared setup status preserves that distinction even if a
  reporting write would fail.
- [ ] Support foreground execution, background launch, and replacing
  `exec`. Define signal dispositions and inherited descriptors explicitly.
- [ ] Implement wait-for-pid and wait-for-any with one child-status owner.
  Preserve the distinction between pid, child status, and wait error.
- [ ] Keep a status reaped by an authored wait available to its owning
  pipeline. Do not wait twice or invent an `ECHILD` failure after a
  successful public reap.
- [ ] Distinguish owned foreground work from background jobs allowed to
  outlive the program. A blanket runtime destructor must not wait for
  every background process.
- [ ] Give forked C++ function stages an explicit child entry/exit
  boundary. They must not unwind through inherited parent ownership or
  run the parent's END by accident.

Exercise launch errors using the native helper and controlled executable
fixtures, not assumptions that a particular system utility exists.

## 4. Implement Linux memfd capture

- [x] Implement general in-process capture with `memfd_create` and
  close-on-exec ownership. Redirect actual fd 1 so helper writes and
  foreground external commands see the same destination.
- [ ] Save stdout, run the C++ body in-process, restore stdout, position
  and read the backing file, and release the frame. Retain the body's
  result separately from its output.
- [ ] Handle early exit and failure at every step. Restore the enclosing
  capture/output destination before reporting or entering END; preserve
  the triggering error if cleanup also fails.
- [ ] Support nested capture, redirects within capture, external commands
  within capture, and capture inside a parent-reader loop.
- [ ] Exercise output substantially larger than pipe capacity, empty
  output, embedded NUL, final newlines, and the agreed background-writer
  completion rule. Do not impose a scratch-buffer limit or silently
  truncate; report allocation/storage failures.
- [x] Report `memfd_create` failure explicitly, including lack of host
  support. Do not silently fall back to a named file or a different
  capture implementation on Linux.
- [ ] Give the backing-storage operation a small replaceable platform
  boundary for macOS. Expose a seekable owned capture store, not the
  Linux syscall throughout the library.
- [x] For an external-only capture, implement the concurrently drained
  pipe path through the process/pipeline machinery. Never wait for the
  writer before draining its pipe.

The memfd path deliberately avoids the deadlock of an in-process body
filling a pipe while nobody drains it. It also avoids changing global
writes by moving the body into a child process.

Do not add a userspace-buffer fast path initially. A captured body probing
stdout sees the memfd's regular-file behavior; a buffer-only path can make
that observation different. Adding it is a separate choice, not an
invisible optimization or a reason to implement an effect analysis here.

## 5. Implement streaming pipelines and parent readers

- [ ] Use one stage-building path for external programs, builtins, and
  C++ function bodies. Check pipe creation, launch, and child fd setup
  before consuming their results.
- [x] Support a terminal parent reader, passed explicitly to its consuming
  operations. Keep updates to ordinary C++ variables visible in the
  parent.
- [x] Retain DD#122's reader-local ownership as the reference behavior:
  the designated read consumes the pipe, while an unrelated command in
  its loop body still inherits the program's stdin. Do not rebind fd 0
  throughout the parent.
- [ ] Give every nested reader its own fd and pending children. Calls,
  captures, redirected commands, and background bodies must not
  accidentally adopt an enclosing reader.
- [x] Start producers, consume concurrently, close the reader, then reap
  and evaluate completion. Exercise scalar early completion as well as
  full-stream and loop consumers.
- [ ] Complete the reader regions left by ordinary completion, `break`,
  escaping `continue`, return, and shutdown, inner first. Preserve the
  selected transfer/result when cleanup succeeds.
- [x] On partial construction failure, close successful endpoints and
  account for already-started stages. Keep the existing close-and-wait,
  no-automatic-force-kill policy unless DG explicitly changes it.
- [ ] Separate status aggregation from execution. Suppressed stages still
  run; apply the selected suppression, negation, and pipefail rules to
  their outcomes rather than omitting stages.
- [ ] Pin SIGPIPE behavior and child signal dispositions, including early
  readers. Do not accidentally inherit a parent-wide SIGPIPE policy into
  every executed program.
- [x] Integrate external capture and memfd-backed compound capture with
  the same child ownership and completion machinery.

Public wait and reader completion must share recorded child outcomes.
Construction failure is not EOF and must not run a consumer on an invalid
descriptor or clear its destination as if an empty stream arrived.
The checked completion and partial-construction items are implemented for
external stages. The common path for builtins/forked C++ stages, escaping
reader control transfers, public wait, and suppression remain open.
Both capture paths now reuse foreground external launch and child ownership;
the integration does not add a second child-status registry.

## 6. Implement shutdown, END, and signals as one lifecycle

Target [DD#5](../decisions/DD05-planned-end-explicit-handling.md), DG's
planned END-only contract, rather than current native DD#56's automatic
cleanup handling. Nothing outside END changes; called function bodies
retain their existing rules.

- [x] Provide one program-entry/shutdown driver with a user-supplied END
  function and ordinary C++ global state.
- [x] Keep source function definitions outside END.
- [x] Demonstrate explicit catch or suppression for every fallible END
  operation. Do not insert automatic END aborts or catch-and-continue
  guards, and do not pass unchecked END state into ordinary functions.
- [x] Exercise normal completion, explicit exit, uncaught operation
  failure, and signal-requested shutdown through that driver.
- [x] Run END at most once. Preserve the triggering status unless END
  deliberately overrides it; END failures follow their explicit handling.
- [x] Close outstanding readers before reaping, restore implemented capture
  state, and enter END on the correct program channels.
- [ ] Keep cleanup ownership separate in forked children. Discard
  inherited reader ownership rather than adopting the parent's children.
- [x] Record signals using signal-safe state only. Perform C++ cleanup,
  diagnostics, and END execution in ordinary execution context.
- [x] Keep most-recent signal information distinct from a pending request
  to exit, so FLAG delivery cannot become an EXIT request accidentally.
- [x] Expose checkpoints that hand-written functions and loops can call.
  Their eventual placement in generated code belongs to the consumer.
- [x] Exercise interrupted reads/waits, FLAG versus EXIT, a signal during
  capture, and a signal while a pipeline is active. Define restart versus
  shutdown behavior without doing allocation or throwing in a handler.
- [x] Handle expected standard-library failures, including allocation
  failure, through the selected program-level policy. The final
  diagnostic path must not depend on allocating another large string.

The remaining unchecked child-ownership item is for future forked C++
stages/jobs. Existing external pre-exec children never unwind inherited
C++ owners; the lifecycle prevents parent END in children, but is not a
new implementation of inherited-reader disposal. General redirects and
public wait remain their own roadmap items.

Do not use `std::exit` or a generic process destructor as a substitute for
the lifecycle: automatic C++ objects, END, foreground children, and
independent background jobs have different completion rules.

## 7. Build value operations in parallel

This track can start after the common result conventions, independently
of most process work. Keep ordinary C++ representations visible.

### Sparse collections - implemented

- [x] Use `std::map<std::int64_t, T>` and
  `std::map<std::string, T>` for string/int indexed arrays and maps.
- [x] Implement DD#6's typed-zero reads and DD#7's rejection of negative
  array indices and population length.
  Use non-inserting lookup for reads; distinguish absence, empty string,
  and integer zero where the selected rules require it.
- [x] Cover assignment, deletion, clearing, key/value traversal, joins,
  splats, sparse indices, and array-to-map conversion.
- [x] Preserve identity through reference parameters and forwarded slots.
  Keep whole-container copying distinct from passing a reference.
  Generic C++ aliases remain safe; source pairs conform to the reference
  producer's distinct-identity/no-overlapping-global DD#100 contract.
- [x] Implement DD#7's live sorted traversal, including mutation and
  deletion through an alias without invalidated-iterator use. Keep explicit
  snapshots independent, and apply sorted-key order consistently, not
  just to `keys`.

### Strings, arguments, and reads

- [x] Use ordinary counted-string concatenation and shared byte length,
  signed/clamped slicing, ASCII case conversion, and nonempty `p?`/`g?`.
- [x] Share those operations between scalar strings and string cells,
  including owned results, binary bytes, and non-inserting missing reads.
- [ ] Complete remaining formatting and applicable text families, keeping
  scalar/string-cell behavior shared rather than divergent.
- [x] Preserve binary bytes internally, and distinguish an empty argument
  from zero arguments. Implement defaults, required-value failures, and
  elision without evaluating unused fallback expressions eagerly.
- [x] Prepare owned command-argument vectors before launch, including
  scalar arguments, sorted splats, lazy defaults/requirements, and explicit
  elision. Allocation failure does not publish a partial splat.
- [x] Provide argv snapshots and environment lookup, including unset
  versus set-to-empty, without mirroring variables into the environment.
- [x] Implement scalar line reading, NUL-separated bulk reading, and
  string splitting. Cover empty records, consecutive/trailing separators,
  final unterminated records, and read error versus EOF.
- [x] Pin replacement-on-read behavior, including clearing at empty input
  and EOF, using DD#99 as the reference rather than reviving retired
  `mapfile`, IFS, or flag-dependent read variants.

### Arithmetic - checked operations and strict text conversion implemented

- [x] Keep integers as `std::int64_t` with DD#8's checked functions and
  native DD#25 overflow/shift rules, without signed C++ undefined behavior.
- [x] Cover add/subtract/multiply, negation, division/remainder edge cases,
  shifts, and exponentiation.
- [x] Keep arithmetic faults distinct from normal boolean outcomes and
  command verdicts under the selected failure policy.
- [x] Implement accountable integer-text input/conversion under DD#77.
  Validation/default/required conversion shares one strict parser for
  scalars and string cells; valid zero and out-of-range disposition remain
  distinct from arithmetic faults. Integer rendering stays `std::to_string`.

### ECMAScript regex

- [x] Construct ordinary `std::regex` explicitly with ECMAScript grammar,
  retaining its normal standard-library locale/flags.
- [x] Implement search, owned whole-match extraction, dense capture arrays,
  and standard repeated group/whole-match extraction.
- [x] Pin no-match results, nonparticipating/empty groups, zero-length
  progression, original anchor context, and counted binary subjects.
  Use length-aware iterator ranges rather than C-string APIs.
- [x] Publish output arrays atomically and reject literal-NUL patterns
  explicitly rather than allow silently incorrect compilation.
- [x] Implement first/global replacement with DD#12's standard C++
  templates, owned normal-miss results, counted formats, and DD#11's
  original context/zero-length progression.
- [x] Convert `std::regex_error` into an explicit operation failure,
  distinct from a successful no-match result. Do not assume a literal
  supplied by a C++ caller can never fail to compile.
- [x] Exercise matching/extraction and replacement with libstdc++ and
  real Apple libc++. There is no linear-time/backtracking-cost guarantee
  or hidden timeout.

### Paths and other OS values

- [ ] Implement globbing, file predicates, path operations, `cd`/cwd,
  process IDs, time, and random values using standard facilities or the
  small platform boundary.
- [ ] Discuss POSIX glob versus filesystem traversal with DG, including
  dotfiles, no-match behavior, sorting, symlinks, and lexical versus
  resolved paths. Regex grammar is not glob grammar.
- [ ] Pin root/trailing-slash and basename/dirname behavior rather than
  assuming a standard-library operation has identical semantics.
- [ ] Distinguish an ordinary negative file predicate from an OS error
  according to the agreed operation policy, without blanket catches.

## 8. Bring up macOS soon after the first Linux slice

Do not wait for every Linux feature or optimization to be finished.

- [x] Once descriptor ownership, one external command, and one memfd
  capture work on Linux, choose and implement the macOS capture-storage
  alternative behind the existing boundary.
- [x] Compile the same shared C++17 code and source-shaped examples on
  macOS, with a real macOS compiler and standard library.
- [x] Exercise all currently implemented launch, capture, fd restoration,
  reader/pipeline, and resource cases on the real Mac.
- [ ] Carry later lifecycle and value-operation cases over as they are
  implemented; this initial port does not complete those features.
- [x] Address differences in descriptor operations, interrupted close,
  signal APIs/numbers, and filesystem behavior locally. Do not scatter
  platform conditionals throughout the shared runtime.
- [x] Keep ordinary fixtures independent of `/proc`, Linux-only `/dev` behavior,
  Linux utility paths, and Linux signal-number assumptions.
- [x] Leave Windows outside this task. Source portability does not imply
  that Unix commands or numeric signals become portable automatically.

## Ordering, evidence, and completion

The critical path is:

**Common interfaces -> descriptor ownership -> process launch ->
one-command capture -> streaming pipelines -> integrated shutdown.**

General memfd capture can start after descriptor ownership; value
operations start after the common interfaces. Integrate both capture paths
with pipeline ownership rather than developing independent child trackers.
Begin macOS bring-up after the first Linux launch/capture slice.

- [ ] Exercise real native operations through small programs, with
  independently stated expected bytes, statuses, state changes, and fd
  ownership. Do not make agreement with Bash or native sash the only
  assertion for a deliberately different behavior.
- [ ] Use handshakes rather than sleeps for concurrency-sensitive cases.
  Keep outer timeouts on executable fixtures; a harness timeout is not
  permission for the runtime to force-kill user processes.
- [ ] Cover large output, nested readers/captures, closed standard
  descriptors, descriptor-number collisions, partial construction,
  early completion, public wait before reader finish, and failure in END.
- [ ] Exercise failure of open/dup/pipe/launch/memfd/read/write/wait using
  controlled resource limits or focused platform-call fault injection.
  Confirm the intended failing operation, not just a matching message.
- [ ] Compile the source-shaped examples with GCC and Clang on Linux;
  run the macOS implementation as it follows. Use sanitizer builds for
  ownership, lifetime, and arithmetic mistakes.
- [ ] Supply harmless counterparts of the real-script roles: an external
  wrapper, a loop mutating state, a pipeline filling a collection, option
  traversal, and END cleanup.
- [ ] Never execute the supplied disk-preparation script or incorporate
  its destructive operations into fixtures.

Completion is a standalone library and a set of readable C++ programs
demonstrating the complete lifecycle, including difficult failure paths.
No part of that demonstration depends on new producer output.

The later consumer still has substantial work: binding/type mapping,
expression evaluation order, source-shaped control flow, call-site result
handling, reader-region exits, and signal-checkpoint placement. Automatic
capture-path selection and source comment preservation also belong there,
not in this runtime task. Native replacement-template translation must
handle evaluated strings and literal dollars/backslashes, not merely
C++ literal escaping; it remains deferred to that consumer.
