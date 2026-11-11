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
author's validation predicate is sufficient. Explicit trust (`:!`) and
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
    signal planning + terminal/entry checks + dead-variable checks
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
| [`signals.rs`](../sash/src/signals.rs), [`terminal.rs`](../sash/src/terminal.rs), [`ssa_taint.rs`](../sash/src/ssa_taint.rs) | Signal checkpoint planning, ERR effect checks and SSA taint verification |
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

**Collections have checked signatures.** Each function has an inferred
collection-argument count and array/map, string/int element signature,
including forwarded slots. Slots alias the caller's collections.
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

SSA versions describe data flow; they do not promise a separate runtime
cell for every version. Source globals use actual static process storage in
both backends, so separately generated handlers can observe completed
writes. Function/capture-owned locals are initialized to meaningful zero
values through the shared body-lowering path, including self-tail entry.
Globals, arguments, slots and active enclosing locals are not fresh locals.

Synthetic variables are allocated through `VersionState::reserve_temp_name`
with their actual type before definition. A prefix or counter alone is not
a collision guarantee: source names beginning with `__` are legal.

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
effects. The SSA verifier separately follows definitions, phis and reaching
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
checks inside the called body or its operand captures.

Function inlining must preserve lexical lifetimes, arguments, collection
slots and return kind. Integer-value calls have a separate result path from
stdout capture; `$()` is not universally a request to collect stdout.
[DD55](../notes/decisions/DD55-error-model.md) and
[DD58](../notes/decisions/DD58-int-function-capture.md) hold the contracts;
their queued revisions must not be mistaken for delivered lowering.

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

The heap is an mmap-backed region managed by
[`heap4.c`](../sash-rt/csrc/heap4.c), a first-fit/coalescing allocator, not a
bump allocator. `--heap-size` supplies a compiled-in reservation limit.
The Plan 9 regex engine is built into the compiler for literal-pattern
validation and into the runtime for matching and substitution; both build
scripts must track its shared C sources and headers.

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
[`terminal.rs`](../sash/src/terminal.rs) checks ERR's operational-failure
handling on executable SSA.

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
