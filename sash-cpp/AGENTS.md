# External C++ transpiler experiment

This project is independent of the Rust compiler and its existing C backend.
Code lives here; journal, tasks, decisions, and status live in `../notes-exp/`.
Do not put experiment records in `../notes/`.

Use C++17, `std::map`, ECMAScript `std::regex`, and Linux memfd capture.
Darwin uses private mkstemp files unlinked before the in-process body;
shared-memory fds cannot service that byte-write contract. Linux has no
temporary-file fallback.
DG makes consequential design decisions; see `../notes-exp/decisions/`.
The runtime task is `../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md`.

Build and exercise changes with `mk -p 1 test` in this directory. Use distinct
`BUILD` directories when changing compilers or sanitizer flags. `mk -p 1
sash-example` additionally compiles all fifteen sash examples with the real
CLI. Ordinary C++ tests do not require a Rust build or IR input.
`mk -p 1 fault-test` adds Linux strace probes; run it without sanitizer
instrumentation. The native command helper is deliberately unsanitized.

The build selects Linux or Darwin automatically. Shared POSIX launch,
I/O, descriptor restoration, and child ownership stay in
`src/platform_posix.cpp`; host adapters only supply their OS primitives.
Darwin pipe flag setup is non-atomic under the selected single-threaded
model and must own both endpoints before either fallible flag operation.
Do not retry ordinary close on either supported host.

Supply the compiler-computed script-fd set at `run` entry, before private
allocation; 0/1/2 are implicit. Private pipes, captures, backups, and diagnostic
copies must skip every reserved number, including closed ones. Their actual
numbers are runtime/OS choices, not IR identities. Temporary redirect
destinations and duplication sources require metadata; a late reservation
of a still-owned private fd is an explicit setup error.

Use `Redirect` factories and the void `redirected` callback for ordered
temporary scopes. Save all affected destinations before file effects,
restore closed state and fd flags, and preserve the body exception.
Earlier truncation/writes are not rolled back. Runtime diagnostics use
saved invoker stderr, never temporarily rebound fd 2; closed invoker stderr
has no fallback. Restore invoker stdio before EXIT or ERR and again on
EXIT -> ERR. Keep source reservations allocation-free and independent of
snapshot construction so startup ERR cannot lose them. The initializer list
is borrowed through the full synchronous `run` call, including handlers.
Persistent exec/rebinding and producer metadata export remain separate work.

Complete fallible private pipe relocation in named owners before returning
an aggregate of noexcept moves. Invoker snapshots are directly initialized
members, not fallible aggregate elements: the used GCC toolchain leaked
earlier owners on a later initialization failure. The fd-limit fixtures
pin both released kernel fds and cleared ownership/reservation state.

Fd-exhaustion fixtures initialize ordinary exception/vptr metadata before
lowering limits. Otherwise this host's GCC UBSan tries to read mappings
through a low fd it cannot relocate and loops inside sanitizer code.
The independent cold/warm control is recorded in journal 021; keep all
sanitizer categories and the real allocation failures enabled.
Redirection fault probes use a child-free baseline and actual injection
records. Their Python assertions ignore inherited optimization settings;
an ineffective-injection negative control must still fail.

Real macOS validation uses Apple Clang/libc++ and `BUILD=build/apple`;
`BUILD=build/apple-asan` with the ordinary ASan flags also works.
Tests require Plan 9 mk and `timeout` on PATH. Linux-only fault/catalog
targets reject Darwin, rather than silently skipping their probes.
Use parenthesized global POSIX signal-set function calls such as
`(::sigemptyset)(&set)` to tolerate Darwin's function-like SDK macros.

`mk -p 1 error-demo` runs the error-path showcase without sanitizers.
`bash examples/error_paths.sh --basic BUILD` needs no strace; `--list`
lists cases and `--case NAME` selects one. Traced cases use a Linux
subreaper so fatal cleanup examples cannot orphan their helper children.
Calibrate fault sites from baseline traces after the entry marker, verify
the actual injected syscall, and keep negative controls capable of failing.

Use the selected single-threaded fork/exec model. Prepare argument storage
before fork; the pre-exec child must not allocate or unwind parent objects.
Keep normal exit, signal termination, and setup failure distinguishable.

External pipelines retain every stage outcome. Checking uses the rightmost
failed stage, not the largest numeric status; setup/cleanup errors are
recorded separately and the first takes precedence (experiment DD#3).
Unstarted stages have no invented exit status. Close every remaining parent
endpoint before waiting for any child, including partial construction and
consumer unwinding.

External-only capture drains a pipe into exact counted bytes and returns
them separately from pipeline outcomes; it does not rebind parent stdio
or depend on in-process capture storage. General in-process capture uses
memfd on Linux and unlinked regular files on Darwin. Checking
precedes assignment; consumer read/allocation exceptions escape even from
the inspectable form instead of becoming EOF or a child verdict.

Command-backed reader callbacks receive an explicit `LineReader &`; do not
rebind parent stdin. Close their pipe before waiting for the producer,
including during unwinding and partial launch failure.

Later-fork/read injection uses a live producer that cannot exit before the
selected entry. strace counts restarted syscall entries too. Higher child
close ordinals use the static supervisor's `--close-prelude`; injecting them
directly into a dynamically loaded parent can fail before `main`.

`tests/capture_faults.sh` selects the first/second positive data read by fd
after the marker, using one finite large binary producer. Its no-memfd
probe first proves injection works on a real memfd capture, then requires
no memfd call from pipe capture; an uncalled syscall was not injected.
The runtime test executable alone overrides allocation to fail a collector
request without exhausting host memory; matching delete overloads remain
paired and the fault is disabled outside its scope.

The storage fixture pins regular-file shape, zero links before body writes,
Darwin privacy, and invalid-TMPDIR failure before body execution. That
invalid directory must not break Linux memfd or external-only pipe capture.

Keep generated-code examples source-shaped: real functions, ordinary control
flow, and standard C++ values. Preserve command outcomes for caught/discarded
uses; checking an outcome is what raises an aborting exception.

Collections remain ordinary `Array<T>`/`Map<T>` standard-map aliases.
The reference producer's 2026-11-02 DD#100 contract requires distinct
collection identities per call and access through passed/forwarded slots,
not overlapping global names during that call. Paired examples conform;
the C++ library retains ordinary references and alias-safety fixtures
rather than adding source-admission checks or copying parameters.
Use `cell`, `set_cell`, and `unset` for source cell accesses: reads are
owned typed-zero values without insertion, and negative array indices fail.
Population length counts stored empty/zero cells.
Use the shared live `values`/map-only `keys` ranges for source iteration,
not a private iterator walk: a body or called alias may erase the current
cell. They retain a copied key and advance via `upper_bound`; loop values
are copies. Explicit key snapshots and completed argument vectors remain
independent values. Do not accept a temporary in a borrowed live range.
Keep scalar/cell dispositions shared and lazy, and preserve empty splat
cells unless the source explicitly elides the entire empty collection.
Prepare owned arguments before launch; NUL is rejected at the existing
exec boundary, not silently removed by collection helpers.

Keep source integers as `std::int64_t`. Use the checked `add`, `sub`, `mul`,
`neg`, `div`, `mod`, `pow`, `shl`, and `shr` functions for fallible arithmetic,
not raw signed operators or a numeric wrapper. ArithmeticError is distinct
from command outcomes; discarding a returned integer never suppresses it.
Retain DD#25 shifts: reject negative counts, never mask large counts,
sign-fill right shifts, and treat discarded left-shift bits as bits rather
than overflow. The implementation uses bounded checks and unsigned bits.
Keep comparisons and lazy `&&`/`||`/`?:` source-shaped, and sequence operands
when their effects or failures make order observable.
`parse_int` is shared by `is_int`, `int_or`, and `int_required` for scalar
strings and string cells. Keep DD#77's complete, signed-i64 grammar and
allocation-free parsing. Invalid or out-of-range input follows disposition,
not ArithmeticError; valid zero must not default. Required messages are
lazy and use the existing status-2 renderer. EXIT remains checked; ERR
must explicitly handle operational failures.

Pass ordinary C argc/argv explicitly to `arg`/`argv`; snapshots own their
strings, include argv0 by default, and reindex slices from zero. Optional
arg defaults are eager and test presence, not emptiness. `getenv` returns
an owned optional string: missing is an ordinary lookup result, never an
automatic abort (DD#101), and present-empty stays present. Reject NUL names
and surface actual allocation/metadata failures rather than inventing absence.
These helpers add no source taint/admission checks or environment mirroring.

String operations take counted values, not variable names. Use the shared
`strlen`/`substring`/`toupper`/`tolower`/`is_print`/`is_graph` helpers for
scalars and string-cell reads. Slice offsets and optional signed lengths
count bytes, clamp bounds, and distinguish omission from explicit zero.
Negative offset counts from the end; negative length trims the remainder.
Do not negate signed minima. Slices and case results own their bytes;
case conversion changes ASCII letters only and preserves NUL/high bytes.
Print/graph require nonempty ASCII `0x20..0x7e` / `0x21..0x7e`; false is
an ordinary result. No locale dependence, implicit trimming, Unicode
normalization, malformed-number repair, or hidden helper checkpoints.

Regex helpers use ordinary compiled ECMAScript `std::regex` values.
`match` searches; `match_text` owns its whole match; out-array `match`
stores whole plus groups, while `matchall` stores flattened groups or
whole matches without groups. Empty/unmatched groups keep their slots.
Subjects are fully counted, including NUL: this is a deliberate difference
from native DD#63's first-NUL subject boundary, not a native bug fix.
Use standard regex_iterator progress and original anchor context, not
native-style suffix searches or immediate one-byte empty-match skips.
Build owned captures locally and swap once: no-match clears the target,
but exceptions preserve it and a target-borrowed subject remains safe.
False is ordinary; regex errors use the existing status-2 exception and
allocation failures escape. Native regcomp failure does not return a
catchable verdict. Reject literal NUL in patterns with an explicit
diagnostic directing to `\x00`; do not silently truncate or rewrite them.
Copy-allocation faults use the host string capacity's exact allocation
size; first prove scalar search and the complete standard iterator walk
succeed under that fault. A broad size threshold can block libc++'s
executor before the intended owned-copy boundary.
No hidden checkpoints or backtracking/resource guarantees.

`sub`/`gsub` return owned first/global replacements using standard C++
notation: `$&`, `$1` through `$99`, `$$`, and per-match prefix/suffix context.
Backslashes remain literal data; computed templates use the same rules.
Keep subjects and templates counted through NUL/high bytes. Use the
standard iterator and explicit-range match formatter: this host's
convenience replacement overload truncated NUL-bearing formats.
Preserve unmatched input and return owned unchanged input on a normal miss,
never as an exception fallback. Destination-borrowed views remain valid
until evaluation completes. Native-template translation belongs to the
future IR consumer, not a runtime adapter (experiment DD#12).
Replacement allocation faults derive their exact size from the completed
output capacity, not the borrowed template's capacity. First prove the
search/complete iterator walk succeeds under that same fault.

NUL reads return owned dense string arrays for counted strings, string
cells, borrowed fds, and command-backed readers. Retain DD#99 terminator
semantics: real empty records survive, a final NUL adds no synthetic field,
and empty input/EOF replaces the old array with empty. Newline/CR/high bytes
remain data; convert other separators upstream. The current source pair
uses `$'\x00'`, not the reference compiler's retired `$'\0'` escape.
Use `LineReader::read()` inside an existing callback so prefetched bytes
are not lost. Raw `read(fd)` borrows and never closes/reflags the descriptor.
The simple bulk collector temporarily holds the full input plus owned
fields and shares the scalar splitter. I/O/allocation errors publish no
partial result, but consumed bytes are not rolled back; producer failure
after completed assignment does not undo that assignment. Existing
close-before-wait cleanup and explicit checkpoint placement remain unchanged.
Field-copy faults use the healthy field's exact allocation size and a
complete small-field control; collector faults start inside the callback,
after launch. A nonblocking pipe with its writer held open pins real
post-prefix EAGAIN rather than manufacturing EOF.

A native undeclared word-loop binder is a string even over an int
collection (DD#86), not automatically the range's C++ value type. Honor
that source binding in a consumer; arithmetic-only pairs can use genuine
counting loops instead of introducing string-to-int parsing.

Function-result discard follows native DD#55's 2026-10-30 no-reach rule:
ignoring a returned int/status must not catch failures inside the function,
its arguments, or nested calls. Conditions leave called bodies checked too.
Keep result handling at the call site, not in an inherited unchecked mode.

Paired source examples also follow native DD#127: single-use captures with
unattested external stdout are attested immediately after capture; shared
stdout-producing functions attest their own data inside the definition.
Do not add caller-side attests for a shared producer or runtime taint checks
to C++ helpers. Preserve the independent golden when adapting source rules.

The current terminal contract is
[DD#15](../notes-exp/decisions/DD15-checked-exit-explicit-err.md), adopting
implemented native DD128 and superseding DD#5. END is removed, not aliased.
Source function definitions are allowed only directly at program top level;
no handler or other nested source scope may contain one.

`run` borrows optional EXIT/ERR callbacks; use `nullptr` for absence, never
substitute the other handler. Bodies may return full-i64 fall-through status
or void for zero. Route by cause and complete integer, not the process byte.
Fall-through, explicit zero, and cooperative signals select EXIT; nonzero
explicit exit and uncaught failure select ERR. Checked EXIT can fail into
ERR once. Successful cleanup preserves entry status; only the final process
boundary truncates it. Fatal arithmetic/heap faults attempt ERR once, even
from EXIT; a fault in ERR terminates without allocating another handler.

Use actual process-global source storage and generated handler functions,
not a hypothetical final-state snapshot. Default-initialize backing storage;
execute source initializer expressions inside the body. Preserve completed
writes and leave interrupted assignments unpublished. Ordinary C++ unwinding
closes readers/captures before dispatch; do not port native linked stacks.
Keep parent handlers out of fork children.

ERR's source operations, operands, setup and called bodies need explicit
failure handling; result discard does not reach into callees. Invalid
unhandled operational host ERR exceptions escape the C++ caller, rather
than receiving automatic cleanup policy. Source admission, incoming-taint
rules and top-level placement belong to the future front end/consumer;
do not add runtime taint checks or a temporary validator.

Signal handlers only store lock-free most-recent/pending-EXIT integers;
do not conflate them or clear the pending request with `clear_signum`.
`checkpoint(deferred)` consumes a compiler-owned decision: defer dispatch,
not receipt, through acquisition/adjacent attest or replacement, including
function returns and reader false/completion edges. Never defer failure or
explicit exit. Polls during either handler cannot redispatch. Placement
belongs in generated/hand-written code, not execute/read/capture helpers;
signal-free IR must emit none. Blocking work retries EINTR, without
automatic cancellation, a latency bound, or force-kill.
