# How sash works

**safebash** is the language; **sash** is its compiler. This guide maps the
implemented compiler and runtime, rather than specifying the language or
cataloguing planned features. Start with the [README](../README.md) for
building and running programs.

The [design decisions](../notes/decisions/) record intended semantics.
Acceptance of a decision does not establish that its implementation has
shipped: [status](../notes/status.md) and [tasks](../notes/tasks/) track that
distinction. The [review checklist](REVIEW.md) describes what to verify
when changing this implementation.

This branch also implements the provisional
[DD136 ERR-call trial](../notes/decisions/DD136-err-call-attempts.md).
Its recovery and colon changes are not adopted on main.

## Tools and trust boundaries

`sash` accepts safebash source, conventionally named `.sb`. It is not a
Bash compatibility frontend. The separate
[`sash-convert`](../sash-convert/) tool parses Bash with stock tree-sitter-bash
and uses ShellCheck findings to assist conversion. Its mechanical rewrites
and `CHECK`/`MANUAL` annotations require review; they do not exempt converted
source from compiler checks.

The root Cargo workspace contains `sash`, `sash-convert` and `sash-expand`.
[`sash-rt`](../sash-rt/) is a separate workspace producing the static runtime
archive. Build instructions and development procedures live in the README
and [AGENTS.md](../AGENTS.md#build-and-test).

Compilation provides static checks, not a sandbox or a proof that an
author's validation predicate is sufficient. Forced reads (`:!`) and
taint-disable switches bypass taint protection rather than validate data.
External programs retain their own behavior and operating-system
permissions. Absolute command paths do not freeze the target filesystem or
bundle those programs into the executable.

Signing, integrity enforcement and reproducible-build infrastructure are
separate deployment concerns. Embedding source makes an artifact inspectable;
it does not authenticate that source or make compilation tamper-evident.

## Compilation pipeline

[`main.rs`](../sash/src/main.rs) delegates to `cli_main` in
[`lib.rs`](../sash/src/lib.rs). The driver owns the sequencing and gates:

```text
safebash source (.sb)
    parse -> Program (AST)
    structural validation
    external command resolution
    lexical binding resolution
    semantic validation + type checking
    AST -> SSA, once, using CheckedTypes
    scratch-write checks
    signal planning
    immutable regex-proof + caught-scratch read checks
    terminal/entry checks + dead-variable checks
        +-- --emit-ir / --manifest: inspection output
        `-- SSA taint verification
              +-- Cranelift -> object -> ld + sash-rt -> static ELF
              `-- --emit-c -> C source
```

The hand-written parser uses a logos lexer and recursive descent. It
produces the AST directly: there is no compiler CST or Bash-coercion pass.
The code calls this AST its **IR**; it is distinct from the later SSA.
Structural refusals precede command resolution so an unsupported form
gets its own diagnostic rather than an error about its leftover tokens.

Command resolution rewrites external command heads to absolute paths before
semantic analysis. Lexical resolution then creates a compiler copy with
distinct binding identities. The pre-binding AST and original source remain
available for artifacts and source-facing names.

`CheckedTypes` is the type checker's result, not a hint to a second inference
pass. Lowering consumes it and builds one `SsaProgram`. Signal planning and
handler checks operate on that program; native and C emission both run the
SSA taint backstop on it, subject to the explicit taint switch.

`codegen::compile_to_object` accepts the already-built program and SSA.
It is **not** a source-validation entry point and must not reconstruct a
different frontend result. Direct backend API tests therefore cannot prove
that a source program passes the real compiler.

### Source map

| Area | Responsibility |
|---|---|
| [`parse/`](../sash/src/parse/) | Tokens, source origins, statements, words, expansions and arithmetic parsing |
| [`ir.rs`](../sash/src/ir.rs) | AST definitions, shared traversal and source-form classification |
| [`resolve.rs`](../sash/src/resolve.rs) | Command resolution and shared call discovery |
| [`bindings.rs`](../sash/src/bindings.rs) | Lexical identities and restoration of source names |
| [`validate.rs`](../sash/src/validate.rs) | Structural and semantic checks, taint summaries and source-level flow |
| [`type_check.rs`](../sash/src/type_check.rs) | Scalar/element types, collection signatures and `CheckedTypes` |
| [`transform/`](../sash/src/transform/) | Typed AST lowering, inlining, control flow, captures and pipelines |
| [`ssa.rs`](../sash/src/ssa.rs) | Values, instructions, blocks, versions and process/handler metadata |
| [`signals.rs`](../sash/src/signals.rs), [`terminal.rs`](../sash/src/terminal.rs), [`ssa_taint.rs`](../sash/src/ssa_taint.rs) | Signal checkpoint planning, readonly ERR-import checks and SSA taint verification |
| [`codegen/`](../sash/src/codegen/), [`emit_c.rs`](../sash/src/emit_c.rs) | Native and C emission from the same SSA |
| [`sash-rt/src/`](../sash-rt/src/) | Runtime ABI, data operations, descriptors, processes and terminal dispatch |

These are responsibility boundaries, not independent copies of the language
model. Shared walkers and classifiers belong at the common representation;
a pass-local copy can silently miss a later variant or fix.

## Identities, types and storage

**Bindings are resolved once.** A source spelling is not sufficient to
identify a local in different function or capture bodies. Later semantic,
type and SSA passes use the resolved identity; diagnostics restore the
author's spelling. [DD86](../notes/decisions/DD86-variable-scope.md) explains
the scope and lifetime contract.

Each write-only `_` discard site gets a fresh identity. `_=$value` still
evaluates and checks its RHS, but separate sinks may have different types
and cannot read one another. Colon no longer disposes of operand values
in the DD136 trial.

**Collections have checked signatures.** Each function has an inferred
collection-argument count and array/map, string/int element signature,
including forwarded slots. Slots alias the caller's collections.
Well-typed whole-collection assignments update those contents without
rebinding identity. Only slot-dependent shape checks wait for this signature;
ordinary-name shape checks retain their earlier diagnostic stage.
[DD100](../notes/decisions/DD100-collection-parameters.md) also requires
distinct argument identities and forbids overlap with globals named by the
callee or its transitive callees. That check is independent of taint switches
and supplies the non-aliasing assumption used by summaries.

**SSA is a block graph, not a source-function tree.** It contains the main
entry, handler entries, process regions, instructions and terminators.
Named functions are inlined; direct self-tail calls become loop edges.
Non-executed function templates support lowering diagnostics and metadata
but are excluded from executable analysis views. Void, status and integer
function results remain distinct classifications.

Arithmetic lazy values lower to actual branches in this shared graph, so
unselected key calls and checks never execute. Internal `TestValue` sets
branch truth without changing source-visible status. Earlier arithmetic
operands are retained before later lifted effects; typed default carriers
evaluate their subject once rather than duplicating it in a ternary.

SSA versions describe data flow; they do not promise a separate runtime
cell for every version. Source globals use actual static process storage in
both backends, so separately generated handlers can observe completed
writes. Function/capture-owned locals are initialized to meaningful zero
values through the shared body-lowering path, including self-tail entry.
Globals, arguments, slots and active enclosing locals are not fresh locals.

Synthetic variables are allocated through `VersionState::reserve_temp_name`
with their actual type before definition. A prefix or counter alone is not
a collision guarantee: source names beginning with `__` are legal.

### Immutable scratch

DD131 defines fresh immutable scratch construction; DG accepted the combined
DD131/DD132 package for main after its separate-branch evaluation.
[`scratch.rs`](../sash/src/scratch.rs) records construction sites against
resolved identities, interpreting the original source spelling rather than
qualified local names. Source checks reject second construction sites,
mutation and initializer self-reads, including helper and collection-slot
reads. A post-lowering write check also sees mutations through mapped aliases.

`LifetimeStart` resets the visible binding before initializer or setup work.
Scalar resets use meaningful zeros; collection resets clear the existing
valid table without allocating. Fillers build scratch collections privately,
then `CollectionPublish` installs the completed table pointer. Thus ERR can
observe the new empty instance after a failed fill, but not the previous
instance or a partial batch. Integer-element collections remain pointer-backed.
The same SSA instructions feed both backends and the taint/handler analyses.

Revisiting one construction site is allowed; skipped sites and zero-trip
loop binders do not reset storage. This adds neither block scope nor automatic
destruction at loop exit. A caught attest permits ordinary scratch reads only
inside its success arm; failure and post-construct uses require explicit
forced reads. The failed instance remains intact and unresolved, without an
ordinary binding's requirement to replace it or leave the path. This does not
relax adjacency, ownership or the separate regex-proof requirement below.

Ordinary array fillers now build private batches too. `CollectionPublish`
carries the previous destination explicitly. The shared runtime checks the
complete append-index range and reserves all destination index/entry capacity
before adding cells. Publication then transfers the private batch's owned
integer keys without allocation, rather than cloning the whole destination.
Reservation can compact or grow physical storage, but failure leaves the old
logical contents intact for ERR. The header address remains stable for aliases.
Both backends reserve the same 40-byte runtime header.
Empty batches change nothing, and EOF/no-match truth belongs to the producer,
not the accumulated population. Assignment, scalar `readline` and map
conversion retain replacement semantics.

Array-to-map declarations and existing-map assignments share one lowering
path, resolving both collection slots before constructing a private map.
Replacement-form `CollectionPublish` installs it only after conversion
completes, preserving an old map on failure and leaving new construction
empty. Ordinary replacement retains unresolved taint; a legal direct
failed-attest replacement uses the existing resolution metadata. A write
through a slot never starts a new lifetime for its caller's scratch binding.

Direct `$(keys m)` loops use an independent key snapshot taken at loop entry,
so source-map mutation cannot change their iteration sequence; body lookups
remain live. Mutable sources allocate a snapshot on each entry. Immutable
scratch maps can retain non-copying traversal, since their sequence cannot
change.

`unset NAME[KEY]` carries a static collection identity and an ordinary
subscript expression through the shared binding, type and taint checks.
Lowering retains all keys in source order before performing any deletion or
whole clear, using the existing typed operand saver and deletion operations.
The target denotes storage, not a content read; key evaluation remains checked.
This needs no new runtime operation or collection layout.

Sparse append uses one past the greatest live nonnegative index, or zero
when none exists, with checked overflow. It does not confuse population
with the next free index. The runtime maintains that index on insertion;
deleting the highest key marks it for one lazy rescan, and clearing or
deleting the last cell resets it. Ordinary repeated appends no longer scan
the growing destination. Integer literal appends and replacements reuse the
checked element types and normal integer-cell encoding. Source and SSA
taint and exceptional-edge analysis retain the old destination's
contribution; appending clean data is not failed-attest replacement.
Transferred keys belong to the destination; the remaining private batch keys,
header and index/entry storage are released, while borrowed value backing is
retained. Destination metadata is replaced and freed only when capacity needs
it, not on every append. This is not general reclamation: other snapshots and
retained strings still cost memory. The matched before/after costs and
failure controls are recorded in
[journal 655](../notes/journal/655-append-reservation-and-index-cache.md).

### Directory selection

Explicit `glob`, `rglob` and bare-word array/append/loop sugar share `GlobFill`
and [`directory.rs`](../sash-rt/src/directory.rs). Its optional directory
operand distinguishes regex selection from coarse glob selection and
participates in canonical SSA reads and failure analysis. The compiler
includes the runtime's small
[`glob_pattern.rs`](../sash-rt/src/glob_pattern.rs) validator, so static and
computed coarse-glob patterns have the same final-component, one-star bound.
Quoted and computed list words remain data; source provenance, not the
presence of a star in the evaluated value, selects sugar.

Each pattern enumerates one directory through public rustix, builds
lexically normalized absolute paths and sorts its own batch bytewise.
`rglob` applies ordinary unanchored regex matching to basenames in its
explicit directory, including matching hidden names but excluding `.`/`..`.
It compiles and owns one regex at a time, freeing it on completion or error.
Regex execution-workspace exhaustion is fatal, not no-match.
All patterns complete before publication; duplicates and pattern order
are retained. Successful emptiness differs from an open/read/getcwd failure.
Handled errors release private paths and descriptors without changing an
ordinary destination. Scratch failure leaves the fresh empty instance.

The variadic pattern tails use ordinary argument-list lowering, retaining
splat boundaries, dispositions and elision. Lowering snapshots each completed
operand before evaluating the next, including `rglob`'s scalar directory.
An empty expanded pattern list succeeds without directory normalization or
enumeration; operand effects and failures still happen. Actual splat element
types, rather than a generic expansion's string type, govern string operands.

The shared proof analysis requires valid immutable regex values, including
possible splat defaults. Present-cell proofs survive byte-preserving quoting,
but copies through forced reads and mutable intermediates do not convey
validity. Compiler-owned literal argument batches can establish static facts;
a source-created array still needs its own successful check.

Earlier mixed-list elements are also retained before later effects.
The shared word-concatenation path retains earlier component values,
including their string conversion.
This does not complete the separately tracked general argument-order work.
Both backends use the same runtime status and publication protocol.

## Taint analysis

Taint is a compile-time quarantine on unresolved reads, not merely a check
at command names or filenames. Runtime `attest` predicates validate values;
compile-time analysis checks where that validation is permitted and which
uses it makes legal.

Environment and program-argument acquisitions are sources. Every fd/pipe
read and kernel-backed capture introduces taint, even when a builtin wrote
the bytes. A compiler-owned buffer copy introduces none and cannot erase
existing taint. Source classification in
[`output_taint.rs`](../sash/src/output_taint.rs) and lowering share
[`capture_purity.rs`](../sash/src/capture_purity.rs); the SSA backstop checks
the transport actually emitted.

Within a live binding, a legal adjacent attest or a clean whole-subject
replacement directly on its failed path resolves taint. Ordinary writes,
unset and self-copy through `:!` do not. New private lifetimes start from
initialized state rather than inheriting expired taint.

Across bodies, independent per-function summaries carry global/slot flows,
reads and output ownership. Formal slots are mapped to actual collections
at calls. Own taint may be attested in its producer, or immediately after
its call when the body has exactly one non-self call site; received taint
cannot be attested inside the receiving body. Site count is structural, not a count
of runtime invocations. Shared-output ownership is an additional condition,
not an exception to kernel reads introducing fresh taint.

Source flow tracks joins, loop heads, break/continue paths and return
effects. DD136 adds separate normal and abandoned effects to the same
per-function summaries, retaining origin ownership and partial-refresh
obligations. An absent abort outcome is not a clean outcome. Recovery also
filters adjacent-acquisition metadata against the abandoned state: a later
acquisition cannot lend its position to an earlier abort. The SSA
verifier separately follows definitions, phis and reaching
collection versions. Main, EXIT, ERR and forked children have separate
executable views; a child's definitions cannot clean the parent's view.

There are documented precision limits: children currently inherit a copy
of the **completed parent view**, not a precise fork-time snapshot, and
some unlowered redirect operands still use completed bare-name lookup.
[DD127's implementation status](../notes/decisions/DD127-taint-across-function-boundaries.md#implementation-status)
owns those boundaries. [DD128](../notes/decisions/DD128-exit-err-adjacent-attest.md)
defines strict resolution and handler entry checks.

## Execution and ownership

### Calls and failure checking

Ordinary external commands execute through direct `execve`, with resolved
paths and explicit argument vectors, not an implicit shell. Their failures
are checked automatically unless handled or explicitly discarded.
Non-void user-function results instead require consumption or a permitted
explicit discard. A caller's condition or suppression does not turn off
checks inside the called body or its operand captures. The explicit
ERR-owned attempt boundary below is the provisional exception, not broader
source suppression.

Function inlining must preserve lexical lifetimes, arguments, collection
slots and return kind. Integer-value calls have a separate result path from
stdout capture; `$()` is not universally a request to collect stdout.
[DD55](../notes/decisions/DD55-error-model.md) and
[DD58](../notes/decisions/DD58-int-function-capture.md) hold the contracts;
their queued revisions must not be mistaken for delivered lowering.

### ERR-owned call attempts (provisional)

ERR's own `: void_helper ARG...` establishes a resultless attempt.
A caught status-function call establishes an attempt with a true/false
continuation, including an otherwise-legal named capture-condition.
Only explicit `fail` and existing unchecked operational results abandon
the complete attempted chain. Arguments and attached redirects belong to
that attempt; helpers' own conditions create no nested boundary.

[`transform/attempts.rs`](../sash/src/transform/attempts.rs) lowers these
sites into `AttemptStart`/`AttemptEnd` and checked exceptional CFG edges.
Failure uses the state before the checked operation, not its normal result.
`RecoverGlobal` reloads completed storage and identifies the possible
abandoned versions. Taint and regex checks follow those same edges;
negation exchanges outcomes without turning an abnormal `fail 0` into a
normal success. Its numeric status remains separate from its false verdict.

[`sash-rt/src/attempts.rs`](../sash-rt/src/attempts.rs) tracks outstanding
descriptor backups and bookmarks reader/capture heads. Recovery deactivates
the boundary, restores scoped descriptors, discards unfinished inner
captures, closes all abandoned readers, then reaps their producers.
Enclosing ERR resources survive, including the capture receiving a named
predicate's partial stdout. Completed writes and committed descriptors
are not rolled back. Children clear attempt ownership.

The ledger does not intercept arbitrary runtime aborts. `:?`, argv-NUL
and invalid-index guards, unconditional slurp/read failures, fatal faults,
explicit `exit` and failed restoration remain terminal. A bare void call
or source `-` establishes no attempt. ERR fall-through still preserves its
original entry status rather than the last recovered status.

### Pipelines and parent readers

Process stages execute in children, while an eligible final reader consumes
in the parent. Forked child regions are not ordinary successors of the
parent's CFG: they cannot return into parent control flow or run its
terminal handlers.

A parent reader owns its input descriptor and producer processes. It
streams before waiting. The reader binding is lexical and is suspended
across calls, captures, redirected commands and background children; it
does not globally rebind stdin.

[`transform/reader_cleanup.rs`](../sash/src/transform/reader_cleanup.rs)
places cleanup on edges leaving reader regions. Cleanup closes the read
end before reaping producers and folds their outcomes under the pipeline's
handling rules. Nested readers finish inner-first. Children discard
inherited reader ownership rather than waiting for the parent's children.
[DD122](../notes/decisions/DD122-pipe-binds-to-the-reader.md) explains this
lifecycle.

### Stdout captures

| Path | Transport |
|---|---|
| Simple unredirected external capture | A one-stage pipeline: the parent drains the pipe before waiting for the child |
| Buffer-eligible inline body | Runtime writes are diverted into a userspace buffer; fd 1 is not rebound |
| Other inline body | fd 1 is temporarily redirected to a memfd while the body executes in-process |

An in-process body cannot generally write to an undrained pipe without
deadlocking once it fills. The memfd path avoids that requirement.
Capture frames form a stack so nested buffer and memfd captures do not
steal one another's output.

The name `capture_purity` means **buffer eligibility**, not freedom from
side effects. An inner external capture may coexist with a buffered outer
body; direct externals, pipelines and redirects disqualify that outer body
under the current conservative predicate. Changing the transport also
changes the taint acquisition boundary, so it is not just a backend
performance choice.

## Runtime and terminal control

The runtime is `#![no_std]` Rust with vendored C regex/allocator code and
architecture-specific startup/jump assembly. It has no libc dependency;
that does not make it entirely Rust or eliminate unsafe boundaries.
Public rustix APIs handle suitable operations. Unsupported process,
raw-descriptor, cwd and signal operations go through
[`linux_syscalls.rs`](../sash-rt/src/linux_syscalls.rs), rather than private
rustix APIs or scattered architecture-specific call sites.

Generated code calls the runtime through a C ABI. Strings use pointer/length
pairs, integers are signed 64-bit values, and arrays/maps share the
`SashTable` implementation. Pointer/length representation does not prove
bounds correctness: allocations, terminators, interior NULs and foreign
interfaces still require explicit handling.

`0?` tests for NUL in the counted contents, excluding the storage terminator.
The native and C backends share its runtime helper and the filename guards:
every filename predicate rejects NUL before lookup, in either binary operand.
These are byte properties, not implicit taint validation.

`$(mkdtemp)` returns a generated pathname directly, using the existing
command-value SSA carrier rather than a stdout capture or new IR form.
Both backends call the same runtime constructor: reserve the terminated
result, complete kernel entropy, and create an exclusive owner-only directory
under fixed `/tmp`, retrying collisions. Its clean provenance and NUL-free
byte bound are independent facts. Ordinary failure aborts before assignment;
handled recoverable failure binds empty text with failure, while an inner
`-` explicitly discards the status. Heap exhaustion stays fatal and precedes
creation.

The heap is an mmap-backed region managed by
[`heap4.c`](../sash-rt/csrc/heap4.c), a first-fit/coalescing allocator, not a
bump allocator. `--heap-size` supplies a compiled-in reservation limit.
The Plan 9 regex engine is built into the compiler for literal-pattern
validation and into the runtime for matching and substitution; both build
scripts must track its shared C sources and headers.

`r?` uses that same engine through a quiet
compilation entry. The runtime rejects counted interior NUL, copies
exactly the candidate's bytes into a terminated buffer, and releases both
that copy and the compiled program. Invalid syntax is false; allocation
failure follows fatal ERR routing. No compiled handle or cache persists.
The shared structural pattern check rejects forced (`:!`) reads with SB5105
before its dynamic-pattern early return. It composes the canonical expression
and statement walkers to cover indices, arithmetic and nested captures;
forced reads of subjects, replacements and predicate candidates remain legal.

[`regex_proof.rs`](../sash/src/regex_proof.rs) checks existing consumers on the
shared, pre-fold SSA, before terminal analysis or inspection-output exits.
SB5106 requires an immutable pattern with a successful `r?` witness on every
reaching path. The worklist converges before reporting diagnostics. It tracks
logical storage identities, copy equality, indexed-read dependencies, present
elements and alternative lexical proof scopes. Writes and lifetime starts
invalidate dependent facts; complete scalar quoting preserves identity, while
byte-changing composition requires validation of the finished value.

Inlined helper and capture control flow carries these facts without adding
source-level regex types. Background/pipeline children inherit at their actual
spawn point; independent handler roots inherit no main-path certificate.
Caught scratch regex permission is limited to the successful lexical arm,
including through copies. The same analysis enforces ordinary reads with
SB5107, using the canonical instruction/value read inventory, including
propagation and separately evaluated indices. Eligible attest-subject reads
remain legal; other predicate operands receive no exemption. `--no-taint`
disables ordinary-read enforcement, but `--no-taint-cst` does not; neither
disables regex proofs.

Logical and negated caught attests share positive-attest branch lowering.
Only the consumed RHS result is unchecked; body and operand failures remain
checked. Independent handlers receive restrictions from their actual incoming
states, not success-arm permissions or regex certificates. The proof/read and
SSA taint analyses share pending-signal tracking. Fresh zero initialization
does not finish an acquisition guard; a caught scratch predicate finishes it
on either outcome, without cleaning failed data or deferring signals forever.
These analyses add no runtime storage or compiled-regex cache.

Scalar regex `match`, `sub` and `gsub` carry text and a matched/no-match
Boolean. Direct capture-conditions consume both;
SB4018 requires explicit discard for text-only calls. Substitution sugar
and `basename`/`dirname` project only text. Shared SSA values carry that
choice, and both backends use an optional status pointer added to the
runtime's existing pointer/length-returning APIs. No compiled pattern handle
persists. [Journal 654](../notes/journal/654-glob-scratch-regex-evaluation.md)
records integrated utility, migration and memory/time evaluation;
[journal 655](../notes/journal/655-append-reservation-and-index-cache.md)
records the subsequent append optimization. The package is accepted.
[Converter migration](../notes/tasks/convert-glob-scratch-regex-migration.md)
remains open: unchanged sequential converted `mapfile` fills can accumulate
where Bash replaces, so converted code still needs review.

Executable redirect indices use shared nested scopes: retain command
arguments, evaluate and open each target in order, then run the body.
Stage work stays in the child after plumbing; final readers stay parent-owned.
Temporary scopes restore descriptors. FD-only `exec` keeps them only when
all opens succeed, then rebinds diagnostic routing once; failure restores
the opened prefix. DD119 still forbids a direct capture filename. This
bounded repair does not complete the general stage-timing or inter-word
capture-order tasks.

EXIT and ERR are private generated callbacks over real global storage,
with their own locals and entry status. Selection is by cause:
fall-through, explicit `exit` (including nonzero) and configured EXIT
signals select EXIT; explicit `fail` (including zero) and uncaught failures
select ERR. EXIT can fail into ERR, but neither handler re-enters, ERR
cannot return to EXIT, and forked children run neither.

Fatal dispatch attempts ERR once, including from EXIT. Dispatch itself
allocates nothing, but a handler's own work can still allocate or fail;
a fatal fault inside ERR terminates directly. EXIT's incoming values and
ERR's conservative incoming-taint obligations are checked as separate
roots, not as continuations of main's last SSA block.
[`terminal.rs`](../sash/src/terminal.rs) checks readonly ERR imports on
executable SSA. SB5101's former failure inventory is removed; unchecked
failure outside an eligible attempt terminates ERR.

Inherited logical bindings are readonly in ERR, including through
same-process helpers, captures and collection aliases. The bound source's
global/call census identifies imports; it does not freeze every global or
prevent ERR-only working state and fresh private locals. The trial also
removes mandatory static ERR NUL proofs. Runtime argv guards and authored
`0?`/filename tests remain, independently of taint. A failed NUL guard
terminates cleanup even inside an attempted call.

Signal delivery records state; generated checkpoints perform EXIT dispatch.
[`signals.rs`](../sash/src/signals.rs) shares one plan across explicit and
backend-emitted polls, deferring dispatch across legal acquisition/attest
resolution without deferring an ordinary failure or explicit termination.

## Backends and inspection

The native backend uses Cranelift to emit an object. The driver invokes
`ld` with the checkout's `sash-rt/target/release/libsash_rt.a` to produce a
static ELF. External commands are runtime dependencies, not linked objects.
The C backend emits source using the same runtime ABI; the spec harness
compiles and links it as a differential oracle. Agreement between backends
cannot exclude a bug in their shared frontend, SSA or runtime.

Native executables embed source, IR and command-manifest text.
`--extract` retrieves these artifacts. `--emit-ir` renders the pre-binding
AST as inert Janet data, not executable Janet code; `--manifest` reports
external dependencies collected during lowering. These inspection exits
follow the frontend checks but do not run the executable backends' SSA
taint gate. Neither inspection output is a proof of executable acceptance.
The trial uses IR schema 4, with an explicit `:best-effort` marker on every
command; an older consumer must refuse it rather than erase recovery semantics.

The verified working native platform is x86-64 Linux. The syscall shim has
both x86-64 and AArch64 implementations, but full AArch64 compiler/runtime
delivery and cross-compilation remain separate work under
[DD129](../notes/decisions/DD129-runtime-syscall-layer.md).
`--sysroot` changes command-resolution checks; it does not select an output
ISA or supply a target runtime and linker.

## Verification entry points

[`spec/`](../spec/) exercises the real CLI and generated programs in native
and C modes, with native-only cases where appropriate. In-crate unit tests
cover internal contracts that source programs cannot exercise. Neither
direct backend tests nor a Bash differential replaces an assertion of the
safebash contract.

`mk test` runs the Rust unit/doc tests. `mk gate` also runs static checks,
the spec sweep and its result accounting, failing on mismatches;
`spec/run.sh` alone is not a failure-reporting gate.
[AGENTS.md](../AGENTS.md#spec-tests) owns the exact commands, prediction
rules, standalone-run environment, negative controls and coverage workflow.
Use those instructions rather than copying a historical test count or
assuming an installed binary represents the working tree.

The retired external suites are preserved under `archive/legacy-suites`;
[journal 621](../notes/journal/621-retire-external-suites.md) records retrieval.
They are not current compiler gates.
