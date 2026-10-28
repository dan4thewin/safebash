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
  - "Linux-specific operations are isolated so the macOS implementation can follow soon"
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

This is not completion of the roadmap. Pipeline and reader ownership,
general redirection, END, signal policy, and the collection-operation
library remain. The sash counterpart compiles and
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

This completes the first Linux launch/capture slice, so the macOS
capture-storage choice can now be taken up. It does not complete
background/public wait, forked C++ stages, call-context propagation,
pipelines, redirects, or the full lifecycle.

## Error-path demonstration - implemented

`cd sash-cpp && mk -p 1 error-demo` runs 103 isolated cases: 52 basic
outcome/exception cases, 38 injected faults, and 13 EINTR recoveries.
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
runtime machinery or whole-function suppression.
[Journal 003](../journal/003-error-path-showcase.md) records the evidence
and limits; the runnable catalog is documented in `sash-cpp/README.md`.

## Starting points selected by DG

| Area | Starting point |
|---|---|
| Platforms | Linux first; macOS follows soon, rather than becoming an indefinite future port |
| Language | C++17 |
| Launch and threading | Single-threaded fork/exec, selected in DD#2 |
| Sparse collections | `std::map`, with numeric keys for indexed arrays and string keys for maps |
| Regex | `std::regex` with `std::regex_constants::ECMAScript` |
| General in-process stdout capture | Linux `memfd_create`, not a named temporary file or a synchronously filled pipe |
| Failure propagation | Exceptions for uncaught failures; retain caught outcomes and explicit suppression rather than making every nonzero command exit throw |
| Generated-program shape | Free functions and ordinary C++ control flow, not a bytecode engine or an SSA dispatcher |

These are external-target choices, not changes to native sash. In
particular, standard-map key order is acceptable instead of native
insertion order. Apply the chosen order consistently to keys, values,
joins, splats, and other collection traversals.

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
| Shutdown and signals | DD#56; runtime signal registration and compiler checkpoint insertion | Signal-safe recording, ordinary-context shutdown, END at most once |
| Explicit wait | DD#126; runtime wait and cached-reader-status helpers | Reaped pid separate from child status, shared ownership when public wait meets pipeline completion |
| Collections, reads, and arguments | DD#79, DD#99, DD#100, DD#123; collection and read helpers | Sparse cells, replacement on read, reference parameters, forwarding, and aliasing |
| Strings and arithmetic | DD#12, DD#25, DD#77, DD#87 | Counted bytes, OS string boundaries, conversions, and defined arithmetic faults |
| Pattern operations and paths | DD#39, DD#63, DD#73; regex, glob, and file-test helpers | Operation families and edge cases, not a requirement to retain the native engines |

The DDs explain the native language and its failure history. Use them as
an inventory of behavior to retain or explicitly replace, not as a blanket
compatibility mandate. A current runtime quirk is not an external-target
requirement merely because a spec records it.

## Choices still requiring discussion

Do not reopen the starting points above. Resolve consequential remaining
forks with DG before building their assumptions throughout the library.

| Choice | Alternatives and tradeoffs | Proposed way to settle it |
|---|---|---|
| Checkedness through calls | Exceptions are selected, but a suppressed function must not merely stop at its first thrown command error. A caller context can select checked operations, or the emitter can make the distinction explicit in generated calls. | Start with command outcomes plus checked wrappers; settle call-context propagation before implementing whole-function or pipeline suppression |
| Collection details | Missing cells can yield meaningful zero values or explicit absence. Negative indices, population versus highest-index length, and snapshot versus live iteration need explicit answers. | Present the native rules beside the simpler standard-map alternatives; keep reads non-inserting under either choice |
| Text and arithmetic | Counted byte strings are economical; Unicode-aware operations need more machinery. Checked arithmetic preserves faults; explicitly defined wrapping is another defensible choice. Locale-sensitive operations can vary between hosts. | Settle byte/character units, case/classification locale, numeric parsing, overflow, and shifts together; raw signed C++ overflow is never an implementation option |
| Regex results and replacement | Source-style backreferences require an adapter; standard C++ replacement notation reduces bespoke logic. Group layout, no-match results, and zero-length iteration remain operation-level choices even with the grammar fixed. | Compare readable examples and prefer standard facilities where they remove rather than conceal complexity |
| Capture completion | Trailing-newline treatment and background writers holding the captured stdout affect which bytes a completed capture yields. | Specify the completion boundary and pin it with direct C++ examples; do not let regular-file EOF accidentally decide the language |
| macOS capture storage | An immediately unlinked temporary file is straightforward but filesystem-backed. POSIX shared memory offers a different backing and lifecycle. | Choose for the macOS adapter when the first Linux memfd slice works; neither is an automatic Linux fallback |

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
- [ ] Implement owned descriptors, duplication, close-on-exec, complete
  writes, bounded reads, and interrupted-operation handling. Do not
  blindly retry `close` after interruption across platforms.
- [ ] Keep short writes, EOF, and read/write errors distinct. Preserve
  embedded NUL and arbitrary byte values.
- [ ] Provide the script-used descriptor set as ordinary runtime input.
  Private backups, pipes, diagnostic copies, and capture descriptors must
  avoid those numbers, including numbers initially closed.
- [ ] Apply redirections left to right. Cover input, output, append,
  read/write, and descriptor duplication, including nested redirects and
  originally closed targets.
- [ ] On partial setup failure, do not run the command; restore descriptor
  state and release successful allocations. Do not claim to undo earlier
  filesystem effects such as truncation.
- [ ] Separate temporary command redirects from persistent `exec`
  redirects. Successful persistent redirects commit and rebind the
  program's channels rather than being undone by a scope destructor.
- [ ] Implement the chosen diagnostic-channel rule, using DD#120 as the
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
- [ ] For an external-only capture, implement the concurrently drained
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
- [ ] Support a terminal parent reader, passed explicitly to its consuming
  operations. Keep updates to ordinary C++ variables visible in the
  parent.
- [ ] Retain DD#122's reader-local ownership as the reference behavior:
  the designated read consumes the pipe, while an unrelated command in
  its loop body still inherits the program's stdin. Do not rebind fd 0
  throughout the parent.
- [ ] Give every nested reader its own fd and pending children. Calls,
  captures, redirected commands, and background bodies must not
  accidentally adopt an enclosing reader.
- [ ] Start producers, consume concurrently, close the reader, then reap
  and evaluate completion. Exercise scalar early completion as well as
  full-stream and loop consumers.
- [ ] Complete the reader regions left by ordinary completion, `break`,
  escaping `continue`, return, and shutdown, inner first. Preserve the
  selected transfer/result when cleanup succeeds.
- [ ] On partial construction failure, close successful endpoints and
  account for already-started stages. Keep the existing close-and-wait,
  no-automatic-force-kill policy unless DG explicitly changes it.
- [ ] Separate status aggregation from execution. Suppressed stages still
  run; apply the selected suppression, negation, and pipefail rules to
  their outcomes rather than omitting stages.
- [ ] Pin SIGPIPE behavior and child signal dispositions, including early
  readers. Do not accidentally inherit a parent-wide SIGPIPE policy into
  every executed program.
- [ ] Integrate external capture and memfd-backed compound capture with
  the same child ownership and completion machinery.

Public wait and reader completion must share recorded child outcomes.
Construction failure is not EOF and must not run a consumer on an invalid
descriptor or clear its destination as if an empty stream arrived.

## 6. Implement shutdown, END, and signals as one lifecycle

- [ ] Provide one program-entry/shutdown driver with a user-supplied END
  function and ordinary C++ global state.
- [ ] Exercise normal completion, explicit exit, uncaught operation
  failure, and signal-requested shutdown through that driver.
- [ ] Run END at most once. Preserve the triggering status unless END
  deliberately overrides it; define behavior when END itself fails.
- [ ] Close outstanding readers before reaping, restore capture/redirect
  state, and enter END on the correct program channels.
- [ ] Keep cleanup ownership separate in forked children. Discard
  inherited reader ownership rather than adopting the parent's children.
- [ ] Record signals using signal-safe state only. Perform C++ cleanup,
  diagnostics, and END execution in ordinary execution context.
- [ ] Keep most-recent signal information distinct from a pending request
  to exit, so FLAG delivery cannot become an EXIT request accidentally.
- [ ] Expose checkpoints that hand-written functions and loops can call.
  Their eventual placement in generated code belongs to the consumer.
- [ ] Exercise interrupted reads/waits, FLAG versus EXIT, a signal during
  capture, and a signal while a pipeline is active. Define restart versus
  shutdown behavior without doing allocation or throwing in a handler.
- [ ] Handle expected standard-library failures, including allocation
  failure, through the selected program-level policy. The final
  diagnostic path must not depend on allocating another large string.

Do not use `std::exit` or a generic process destructor as a substitute for
the lifecycle: automatic C++ objects, END, foreground children, and
independent background jobs have different completion rules.

## 7. Build value operations in parallel

This track can start after the common result conventions, independently
of most process work. Keep ordinary C++ representations visible.

### Sparse collections

- [ ] Use `std::map<std::int64_t, T>` and
  `std::map<std::string, T>` for string/int indexed arrays and maps.
- [ ] Implement the agreed missing-cell, negative-index, and length rules.
  Use non-inserting lookup for reads; distinguish absence, empty string,
  and integer zero where the selected rules require it.
- [ ] Cover assignment, deletion, clearing, key/value traversal, joins,
  splats, sparse indices, and array-to-map conversion.
- [ ] Preserve identity through reference parameters, forwarded slots,
  and two parameters aliasing the same collection. Keep whole-container
  copying distinct from passing a reference.
- [ ] Pin snapshot versus live traversal, including mutation and deletion
  through an alias. Apply sorted-key order consistently, not just to
  `keys`.

### Strings, arguments, and reads

- [ ] Implement counted-string concatenation, slicing, case conversion,
  numeric parsing/formatting, and the chosen character predicates.
- [ ] Cover the same applicable operations for scalar strings and string
  cells rather than maintaining divergent implementations.
- [ ] Preserve binary bytes internally, and distinguish an empty argument
  from zero arguments. Implement defaults, required-value failures, and
  elision without evaluating unused fallback expressions eagerly.
- [ ] Provide argument snapshots and environment lookup, including unset
  versus set-to-empty, without mirroring variables into the environment.
- [ ] Implement scalar line reading, NUL-separated bulk reading, and
  string splitting. Cover empty records, consecutive/trailing separators,
  final unterminated records, and read error versus EOF.
- [ ] Pin replacement-on-read behavior, including clearing at empty input
  and EOF, using DD#99 as the reference rather than reviving retired
  `mapfile`, IFS, or flag-dependent read variants.

### Arithmetic

- [ ] Implement the chosen integer width, overflow, conversion, and shift
  rules without relying on signed C++ undefined behavior.
- [ ] Cover add/subtract/multiply, negation, division/remainder edge cases,
  shifts, exponentiation, and integer-text conversion.
- [ ] Keep arithmetic faults distinct from normal boolean outcomes and
  command verdicts under the selected failure policy.

### ECMAScript regex

- [ ] Construct `std::regex` explicitly with ECMAScript grammar and the
  selected locale/flags; do not rely on an incidental default elsewhere.
- [ ] Implement search, whole-match extraction, capture arrays, repeated
  matches, first replacement, and global replacement.
- [ ] Pin no-match results, nonparticipating groups, empty matches,
  zero-length progress, replacement escaping/backreferences, and binary
  subjects. Use length-aware iterator ranges rather than C-string APIs.
- [x] Convert `std::regex_error` into an explicit operation failure,
  distinct from a successful no-match result. Do not assume a literal
  supplied by a C++ caller can never fail to compile.
- [ ] Exercise the selected behavior with libstdc++ and libc++ as their
  hosts become available. Account for backtracking/resource costs rather
  than promising linear-time matching or inventing a hidden timeout.

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

- [ ] Once descriptor ownership, one external command, and one memfd
  capture work on Linux, choose and implement the macOS capture-storage
  alternative behind the existing boundary.
- [ ] Compile the same shared C++17 code and source-shaped examples on
  macOS, with a real macOS compiler and standard library.
- [ ] Exercise launch, capture, fd restoration, and one parent-reader
  pipeline first; then carry the full lifecycle and operation cases over.
- [ ] Address differences in descriptor operations, interrupted close,
  signal APIs/numbers, and filesystem behavior locally. Do not scatter
  platform conditionals throughout the shared runtime.
- [ ] Keep fixtures independent of `/proc`, Linux-only `/dev` behavior,
  Linux utility paths, and Linux signal-number assumptions.
- [ ] Leave Windows outside this task. Source portability does not imply
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
expression evaluation order, source-shaped control flow, call-site failure
context, reader-region exits, and signal-checkpoint placement. Automatic
capture-path selection and source comment preservation also belong there,
not in this runtime task.
