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
sash-example` additionally compiles all ten sash examples with the real
CLI. Ordinary C++ tests do not require a Rust build or IR input.
`mk -p 1 fault-test` adds Linux strace probes; run it without sanitizer
instrumentation. The native command helper is deliberately unsanitized.

The build selects Linux or Darwin automatically. Shared POSIX launch,
I/O, descriptor restoration, and child ownership stay in
`src/platform_posix.cpp`; host adapters only supply their OS primitives.
Darwin pipe flag setup is non-atomic under the selected single-threaded
model and must own both endpoints before either fallible flag operation.
Do not retry ordinary close on either supported host.

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
lazy and use the existing status-2 renderer, with no automatic END handling.

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

A native undeclared word-loop binder is a string even over an int
collection (DD#86), not automatically the range's C++ value type. Honor
that source binding in a consumer; arithmetic-only pairs can use genuine
counting loops instead of introducing string-to-int parsing.

Function-result discard follows native DD#55's 2026-10-30 no-reach rule:
ignoring a returned int/status must not catch failures inside the function,
its arguments, or nested calls. Conditions leave called bodies checked too.
Keep result handling at the call site, not in an inherited unchecked mode.

The planned END-only contract is
[DD#5](../notes-exp/decisions/DD05-planned-end-explicit-handling.md): no
source function definitions inside END, and every fallible END operation
explicitly caught or suppressed. No automatic END abort/suppression guards;
ordinary function bodies and all non-END behavior stay unchanged. This
source contract is not yet enforced by the native compiler or a consumer.
The hand-written lifecycle pair demonstrates it; invalid unhandled END
exceptions escape the C++ caller rather than receiving automatic policy.

`run` owns END and typed `ExitRequest` handling. Signal handlers only store
lock-free most-recent/pending-EXIT integers; do not conflate them or clear
the pending request with `clear_signum`. Checkpoints belong explicitly at
hand-written/generated command, function, and loop boundaries, not hidden
inside execute/read/capture helpers. Blocking work retries EINTR; there is
no automatic cancellation or force-kill. END's checkpoint re-entry guard
does not change ordinary command/function failure checks.
