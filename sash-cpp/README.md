# sash-cpp

An independent C++17 runtime for a future source-shaped sash transpiler.
There is no IR reader or emitter yet. The examples are hand-written:
[greetings.cpp](examples/greetings.cpp) corresponds to
[greetings.sash](examples/greetings.sash), retaining a function, sparse
collection, loop, regex condition, and stdout capture.

[job_report.cpp](examples/job_report.cpp) and
[job_report.sash](examples/job_report.sash) add a bounded while loop,
external commands inside a captured function, a command used as a
condition, and explicitly discarded command results. Two rounds report
three workers and skip a fourth record. Expected bytes live beside each
pair in its `.expected` file.

[stream_report.cpp](examples/stream_report.cpp) and
[stream_report.sash](examples/stream_report.sash) add an external producer
feeding a parent-side `while readline`. A function fills its caller's
sparse collection by reference, captures labels inside the loop, and
leaves the saved workers available afterward. Four records produce three
workers at indices 1, 2, and 4 and a nine-line report.

[pipeline_report.cpp](examples/pipeline_report.cpp) and
[pipeline_report.sash](examples/pipeline_report.sash) put three external
stages before that reader: printf, C-locale sort, and uniq. The function
retains three workers from four unique records; a later condition detects
an upstream failure even when the last command succeeds. Both produce
the adjacent ten-line expected report.

[capture_report.cpp](examples/capture_report.cpp) and
[capture_report.sash](examples/capture_report.sash) capture a five-stage
external pipeline in a real summary function, nest it inside a framed
function capture, and retain two rounds of summaries for later traversal.
The sash failure probe saves status with `||`, attests once, then branches;
the C++ probe keeps captured bytes separate from pipeline outcomes.
Both produce the adjacent eight-line expected report.

[lifecycle_report.cpp](examples/lifecycle_report.cpp) and
[lifecycle_report.sash](examples/lifecycle_report.sash) retain two workers
from a captured function, discard a separate function's returned int,
and explicitly exit into checked EXIT. EXIT catches one failed command,
discards another, and traverses the actual global collection; a separate
ERR explicitly handles its report result. All source functions are defined
directly at top level. Both reports match the adjacent eight-line golden.

[option_report.cpp](examples/option_report.cpp) and
[option_report.sash](examples/option_report.sash) add sparse cell operations,
forwarded collection parameters, population length, joins,
pairwise array-to-map conversion, independent key snapshots, and argument
construction. The report retains empty arguments while explicitly eliding
others; its real argument-reporting function invokes external printf.
Both match the adjacent seventeen-line golden.

[arithmetic_report.cpp](examples/arithmetic_report.cpp) and
[arithmetic_report.sash](examples/arithmetic_report.sash) retain an
int-returning subtotal function, a counting `for`, a `while`, and ordinary
branches. Checked integer functions handle nested arithmetic, negation,
division/remainder, powers, and shifts without changing the integer type.
Both match the adjacent ten-line golden.

[numeric_report.cpp](examples/numeric_report.cpp) and
[numeric_report.sash](examples/numeric_report.sash) add owned argv snapshots,
optional environment lookup, strict string-to-integer conversion, a real
charge function, and ordinary validity and traversal branches. With arguments
`08 0` and `SASH_CPP_NUMERIC_RATE=0x7`, both match the adjacent eight-line golden.

[string_report.cpp](examples/string_report.cpp) and
[string_report.sash](examples/string_report.sash) add byte length/slicing,
ASCII case conversion, and nonempty print/graph predicates. A real option
function handles `COUNT=08`: only the key becomes `count`, while unchanged
`08` converts strictly to eight. String cells use the same operations.
Both match the adjacent seven-line golden.

[regex_report.cpp](examples/regex_report.cpp) and
[regex_report.sash](examples/regex_report.sash) add whole-match extraction,
capture arrays, repeated group extraction, and group-free matching. A real
function extracts option pairs, creates a map, and converts their values.
Both match the adjacent nine-line golden.

[rewrite_report.cpp](examples/rewrite_report.cpp) and
[rewrite_report.sash](examples/rewrite_report.sash) add first/global regex
replacement using equivalent C++/native templates. A real function retains
the match branch, literal dollar/backslash output, and string-cell traversal.
Both match the adjacent ten-line golden; no template translator exists yet.

[record_report.cpp](examples/record_report.cpp) and
[record_report.sash](examples/record_report.sash) add NUL-separated bulk
reads and counted-string splitting. A real function converts `|` upstream,
retains an empty field, reads external NUL records into its caller's array,
and clears the old array with empty input. Both match the adjacent ten-line
golden.

[file_report.cpp](examples/file_report.cpp) and
[file_report.sash](examples/file_report.sash) add scoped input/output/append
redirects, nested function calls, both descriptor-duplication orders, and
an explicitly declared fd 3. Real functions write and collect the files;
a caught missing-input failure leaves stdout restored. Both match the
adjacent eleven-line golden in a private, cleaned test directory.

[terminal_report.cpp](examples/terminal_report.cpp) and
[terminal_report.sash](examples/terminal_report.sash) add checked EXIT to
ERR progression. A real function updates global state and exits with 256
inside EXIT's capture. ERR sees that completed write but not an interrupted
assignment's private output; its incoming status is 256 although the
process byte is zero. Both match the adjacent four-line golden.

On Linux or macOS, with a C++17 compiler, Plan 9 `mk`, and `timeout` on PATH:

```sh
cd sash-cpp
mk -p 1 test
./build/greetings
./build/job_report
./build/stream_report
./build/pipeline_report
./build/capture_report
./build/lifecycle_report
./build/option_report
./build/arithmetic_report
SASH_CPP_NUMERIC_RATE=0x7 ./build/numeric_report 08 0
./build/string_report COUNT=08
./build/regex_report 'COUNT=08;EMPTY=;RATE=0x7'
./build/rewrite_report 'COUNT=08;RATE=7'
./build/record_report
bash tests/file_report.sh ./build/file_report \
  examples/file_report.expected build/file_report.actual
./build/terminal_report
```

`greetings` prints `Hello, Ada!`; `job_report` prints the worker rounds;
`stream_report` prints streamed records and the retained worker list;
`pipeline_report` prints sorted, deduplicated records and retained state;
`capture_report` prints comma-joined summaries and their captured frames.
`lifecycle_report` prints the captured count and checked EXIT report.
`option_report` prints the sparse options, copied keys, and exact argument list.
`arithmetic_report` prints a weighted subtotal, arithmetic results, and a countdown.
`numeric_report` prints converted options, defaults, cell values, and the charge.
`string_report` prints the sliced option and ASCII string-cell results.
`regex_report` prints whole/captured matches, extracted pairs, and converted options.
`rewrite_report` prints first/global rewrites, literal template bytes, and rewritten cells.
`record_report` prints split fields, owned stream records, and replacement-on-read results.
`file_report` reads back redirected files and demonstrates ordering and restoration;
use its harness above to provide and clean the required work directory.
`terminal_report` prints full-width routing and the actual retained global state.
The build produces a normal C++ static library, `build/libsash_cpp.a`;
it does not build Rust or link sash-rt.
For example, the C++ program can also be linked directly:

```sh
c++ -std=c++17 -Iinclude examples/greetings.cpp build/libsash_cpp.a -o build/greetings
```

Use separate output directories for different compilers and flags:

```sh
mk -p 1 CXX=g++ BUILD=build/gcc test
mk -p 1 CXX=clang++ BUILD=build/clang test
mk -p 1 'CXX=g++ -B/usr/bin' BUILD=build/asan \
  'SANITIZE=-fsanitize=address -fno-omit-frame-pointer' test
```

The ASan command selects the system linker as well as GCC on this Linux
development host; mixing its GCC sanitizer libraries with Homebrew's linker
fails. Full UBSan execution has a separate zero-fd fixture/tooling conflict,
recorded in [the sanitizer task](../notes-exp/tasks/sanitizer-host-compatibility.md).

On macOS, the real Apple Clang/libc++ build and its ASan variant use:

```sh
mk -p 1 CXX=clang++ BUILD=build/apple test
mk -p 1 CXX=clang++ BUILD=build/apple-asan \
  'SANITIZE=-fsanitize=address -fno-omit-frame-pointer' test
./build/apple/capture_report
```

Both hosts run the same 73 fixture groups and fifteen expected C++ reports,
including large binary output, nested captures/readers, closed standard
descriptors, partial setup, child/fd cleanup, typed exit, main-only EXIT/ERR,
and signal delivery during foreground work and capture. Collection fixtures
exercise live insertion/deletion through aliases, copied loop values, lazy
dispositions, binary joins, and allocation failures before argument publication.
Arithmetic fixtures cover signed limits, every multiplication sign pair,
division/remainder faults, bounded power evaluation, defined shifts, ignored
function results, failed conditions/arguments, and fatal-fault ERR entry.
Input fixtures cover complete signed parsing, owned argv/environment values,
present-empty versus missing inputs, valid zero, lazy required messages,
scalar/cell parity, and allocation and lifecycle failure boundaries.
Text fixtures cover counted binary bytes, signed/clamped substring bounds,
ASCII-only case changes, nonempty shape predicates, owned results,
scalar/cell parity, and allocation failure without replacing a destination.
Regex fixtures cover counted subjects, owned captures, unmatched/empty groups,
normal misses, standard zero-length progress, original anchor context,
target-borrowed inputs, and allocation failure without partial publication.
Replacement fixtures cover first/global scope, standard C++ templates,
counted binary formats, original match context, empty/nonempty retries,
owned miss results, and allocation failures before caller assignment.
NUL-read fixtures cover real empty records, final terminators and tails,
buffered line-to-bulk handoff, borrowed stdin/fds, binary data beyond pipe
capacity, post-prefix I/O failure, allocation failure, and producer cleanup.
Redirection fixtures cover ordered/nested scopes, file flags and umask,
source-fd reservations, private-fd inheritance, originally closed targets,
partial construction, caught-body restoration before the main boundary,
invoker diagnostics, terminal channels, and bounded descriptor lifetime.
Terminal fixtures cover full-i64 cause-based routing, missing versus empty
handlers, checked EXIT failures, one-shot arithmetic/heap dispatch, actual
completed globals, interrupted captures, and protected signal windows.
They also pin allocation-free startup ERR with surviving source reservations,
a second invoker restoration, and no parent handlers in raw fork children.
The Darwin build has been
exercised on Apple Silicon; this is not a claim of testing every Mac SDK
or architecture. Tests need GNU `timeout`, which is not a stock macOS tool.
The Linux fault/catalog targets below explicitly reject Darwin.

Additional Linux fault probes require `strace` and an ordinary,
non-sanitized build:

```sh
mk -p 1 CXX=g++ BUILD=build/gcc fault-test
```

They exercise failed first/later forks, interrupted wait, recovered wait
errors with retained child outcomes, partial error-pipe setup,
exec-error reporting without child writes, and raw capture read/cleanup.
Six lifecycle checks inject signal installation in the body or ERR,
mask setup within ERR, fatal disposition restoration, and lifecycle
entry/restoration handoff failures with allocation denied. ERR catches
its setup faults explicitly; failed restoration remains a broken-runtime
guarantee, not a suppressed source command.
Capture probes calibrate the first and second positive data reads by fd,
verify retained bytes after a wait error, and prove independence from memfd
with an injection canary followed by an absence check. The probes check the
observed `clone`/`wait4` syscalls and trace evidence, not just a matching
surface diagnostic. Fourteen redirection controls cover setup, inspect,
save, open, install, close, frame/invoker restoration, fatal unwind
restoration, and EINTR recovery. Their baseline has no child, and their
assertions remain enabled even under `PYTHONOPTIMIZE`.
Keep this target separate from sanitizer runs under
ptrace. The command helper is an ordinary unsanitized external executable;
the library, test driver, and examples receive `SANITIZE`.

With the real sash CLI and runtime already built in the parent checkout,
`mk -p 1 sash-example` compiles and runs all fifteen sash examples against
their expected output. Override `SASH` and `SASH_RT_LIB` for other locations.
This additional target is not a prerequisite for native C++ tests.

For DG's existing native compiler and its companion runtime:

```sh
mk -p 1 SASH="$HOME/src/safebash/_tmp/sash" \
  SASH_RT_LIB="$HOME/src/safebash/sash-rt/target/release/libsash_rt.a" sash-example
```

## Error-path showcase

[error_paths.cpp](examples/error_paths.cpp) demonstrates the current
runtime's failure handling, one operation at a time. The Linux runner
isolates each invocation, displays stdout, stderr, and exit status, and
continues to the next demonstration. It succeeds only when the observed
outcomes match its assertions.

```sh
mk -p 1 error-demo
```

The full catalog has 169 cases: 85 basic cases, 65 injected-fault cases,
and 19 interrupted-operation recovery cases.
The newer arithmetic/lifecycle/redirection fixtures and their
injected-fault checks belong to `test` and `fault-test`, not this
command/capture catalog.

The [low-level tour](docs/error-path-tour.md) follows representative cases
through the Bash runner, strace fault injection, C++ outcomes, and resource
cleanup. It assumes C and Unix experience rather than C++ familiarity.

| Area | What is demonstrated |
|---|---|
| Command outcomes | Checked, caught, and discarded failures; exit 7; real exit 127 versus failed exec; signal termination |
| Input and library errors | Interior NUL, missing or non-executable commands, ENOEXEC, invalid descriptors, malformed ECMAScript regex, string capacity, allocation exception |
| Capture | Failed inspection, memfd creation, fd relocation/backup, installation, restoration and flags, rewind, read, and close; initially closed stdout |
| Streaming input | Output/completion pipe allocation, partial launch cleanup, child stdout installation, reader errors/close, and interrupted reads |
| External pipelines | Every stage's exit/signal, rightmost checked failure, unstarted stages, late setup failures, stdin wiring, endpoint ownership, and retained outcomes after parent errors |
| Raw pipeline capture | Exact binary bytes, failed producer output retained with outcomes, unchanged checked targets, pipe allocation/read/wait errors, and first/second read recovery |
| Launch and wait | Pipe and shared-status allocation, fork, child pre-exec close, completion reads/protocol, endpoint close, unavailable child status, wait failure |
| Fatal cleanup and diagnostics | Restore, close, reap, and unmap failures during cleanup; stderr failures while reporting the original error |
| Recovery | EINTR at each implemented retry loop, preserving the uninjected output and status |

Checked captures demonstrate that partial output does not replace the
assignment target; inspectable pipeline capture instead retains the bytes
with failed producer outcomes. Caught and discarded operations continue; checked
failures reach the program boundary instead. Fatal cleanup cannot be
suppressed: it diagnoses the broken ownership/restoration guarantee and
terminates with status 2. This exposes existing runtime policy; it does
not add source-level function-result handling.

On Linux, after `mk -p 1 test`, the basic subset needs neither strace nor the
Linux fault supervisor. Individual cases can also be selected:

```sh
bash examples/error_paths.sh --basic build
bash examples/error_paths.sh --list
bash examples/error_paths.sh --case cleanup-restore build
./build/error_paths exit7 caught ./build/command-helper
```

The full target requires Bash, `strace`, and `timeout`, and must use an
unsanitized build. Raw output and traces stay in `$BUILD/error-demo/`.
Fault locations are calibrated against an uninjected run after an explicit
entry marker, rather than assuming dynamic-loader syscall counts.
The child-close probe uses per-tracee first-close injection, which also
affects the parent's first loader close; both injections are shown, and
the trace must prove the actual fork child failed before exec.
Higher child-close ordinals use the static supervisor's controlled close
prelude so an earlier parent loader failure cannot prevent the intended
child from being created. The full catalog requires static development
archives for this small Linux helper; ordinary examples remain normal
C++ programs and do not require static linking.
A Linux subreaper supervises traced cases so even the fatal-wait example
does not leave an orphaned fixture behind.

`bad-alloc-injected-*` deliberately throws `std::bad_alloc`; it does not
exhaust the shared host. The separate runtime fixture fails a real collector
allocation using a test-executable-only allocation override, then checks
descriptor and child cleanup. It applies no host-wide memory limit.
`completion-protocol-synthetic` injects an
unexpected read count. Arbitrarily corrupted internal shared status and
uncaught programming errors are not forced. Nor does this catalog cover
runtime features that do not exist yet. There is no sash counterpart:
many cases are API misuse or OS faults, not accepted source programs.

## Current boundary

`write` and `echo` check their command outcomes and raise `CommandError`
on failure. `try_write` and `try_echo` return a `CommandResult` without
printing a diagnostic:

```cpp
if (auto result = sash::try_echo({"ready"}); result.success()) {
    // The operation succeeded.
}
(void)sash::try_write("ignored result"); // Explicit discard of one command.
sash::echo({"checked"});                // Uncaught failure aborts through run().
```

`try_execute(path, arguments)` returns a `ProcessResult` with distinct
`ProcessExit`, `ProcessSignal`, and `SetupError` alternatives. Only a
normal exit zero succeeds. `execute` and `check` raise the corresponding
exception; checking a signal result encodes it as `128 + signal`, while
the raw outcome retains the actual signal. Failed exec is not exit 127.
Caught setup failures do not print runtime diagnostics, and a child's own
stderr is still inherited.

Launch uses single-threaded fork/exec with prebuilt argument storage:
no PATH lookup, shell parsing, or ENOEXEC shell fallback. The child does
not allocate or unwind parent objects. Private capture and completion
descriptors are close-on-exec and avoid standard and declared source
descriptor slots, including slots initially closed. The foreground caller
reaps its child on ordinary completion and error paths; cleanup does not
force-kill or set a runtime timeout.

Discard a function's returned int/status at the call site; do not catch its
body failures merely because its answer is ignored. Function bodies and
argument evaluation keep their own checks, including under conditions
(DG's 2026-10-29/30 native DD#55 ruling). No inherited unchecked mode is
needed for this. Per-stage result suppression, background launch, and
replacing `exec` are not implemented yet.

### Temporary redirections and invoker channels

Supply every nonstandard script-referenced descriptor at program entry:

```cpp
int main() {
    return sash::run("report", {3}, [] {
        sash::redirected({sash::Redirect::write("report.txt", 3)}, [] {
            sash::write("ready\n", 3);
        });
    });
}
```

Metadata overloads also accept optional EXIT/ERR callbacks. Forms without
metadata supply an empty explicit set; 0, 1, and 2 are always reserved.
The synchronous driver borrows and validates the initializer list without
allocation; its reservation scope outlives failed snapshot construction
and both handlers. Duplicate/unsorted entries do not change membership.
The future producer should export its already-computed source-fd set.
Private backup, capture, pipe, and diagnostic numbers are runtime/OS
allocations, not compiler-assigned identities to encode in the IR.
Undeclared redirect destinations/duplication sources and late reservations
of already-owned private descriptors fail explicitly.

| Factory | Effect |
|---|---|
| `Redirect::read(path, fd = 0)` | Open existing input |
| `Redirect::write(path, fd = 1)` | Create or truncate output |
| `Redirect::append(path, fd = 1)` | Create or append output |
| `Redirect::read_write(path, fd = 0)` | Open/create read-write without truncation |
| `Redirect::duplicate(destination, source)` | Duplicate the source's current channel |

`redirected({specifications...}, void_body)` applies already-owned paths
left to right. Thus `write(path), duplicate(2, 1)` differs from
`duplicate(2, 1), write(path)`. All metadata and NUL-bearing paths are
validated and every affected destination is saved before file effects.
Creation uses 0666 filtered by the caller's umask. Normal completion and
body unwinding restore the prior destination, closed state, and descriptor
flags, including within nested captures/functions.
Setup failure runs no body; earlier truncation or writes are not undone.
Ordinary completion errors throw `SetupError`; failed unwind restoration
retains the existing fatal cleanup policy.

`run` saves invoker stdin/stdout/stderr after installing the reservation
set. Runtime diagnostics and fatal cleanup use the saved invoker stderr,
not temporarily redirected fd 2; a closed invoker stderr has no fallback.
Invoker channels are restored before either handler and again for EXIT -> ERR.
Ordinary channel initialization failure runs neither body nor handler;
failed invoker restoration prevents the selected callback. A heap fault
after valid source metadata is installed attempts ERR with those
reservations still active, even if invoker snapshot construction failed.
The output helpers use raw writes; buffered stdio/iostream flushing
remains the caller's responsibility at descriptor boundaries.
Persistent `exec` redirects, channel rebinding, and IR consumption remain
separate work. [DD#14](../notes-exp/decisions/DD14-temporary-redirections.md)
records this contract.

### Collections and owned arguments

`Array<T>` is an alias for `std::map<std::int64_t, T>`; `Map<T>` is
`std::map<std::string, T>`. Source element types use `std::string` or
`std::int64_t`. These remain ordinary containers and reference parameters,
not handles into a runtime variable table. Whole-container assignment copies;
passing a C++ reference, including aliases or forwarded references, does not.
The current native DD#100 contract requires distinct collection identities
per call and access through slots rather than overlapping global names
during that call. The paired examples conform; the C++ library retains
ordinary alias-safe references rather than recreating source validation.

Use `cell(collection, key)` for non-inserting reads, `set_cell` for
assignment, `unset` for cell deletion, and ordinary `.clear()` for clearing.
`cell` returns an owned value: `""` for a missing string or `0` for a
missing integer. `len` counts present cells, including stored empty/zero
values, not the highest array index plus one.
Negative array indices raise `SetupError`; string map keys such as `"-1"`
remain valid. These helpers enforce source cell rules; direct C++ container
access is not a substitute for them in emitted code.

`values(collection)` and map-only `keys(map)` are live sorted ranges:

```cpp
for (const auto &value : sash::values(items)) {
    sash::checkpoint();
    sash::echo({value});
}
```

They borrow an lvalue container, copy the current key, and advance with
`upper_bound` after the body. Values and key loop variables are copies too,
so erasing the current cell cannot leave a dangling loop variable.
Later insertions are visited, earlier/current keys are not revisited,
deleted pending cells are skipped, and pending value updates are observed.
Continually inserting later keys can extend the walk indefinitely.
Do not replace this with a plain map range-for when the body can erase
the current cell. Temporary containers are rejected by the live-range API.
Collection helpers do not insert lifecycle checkpoints.

`snapshot_keys(map)` returns a separate dense string array, unchanged by
later map mutations. `join(collection, separator)` joins sorted values,
rendering integers and preserving counted binary bytes. Its default
separator is a space; an explicitly empty separator stays empty, and there
is no fixed-size result buffer.

`map_pairs_elided(array)`, `map_pairs_or(array, key_callback)`, and
`map_pairs_required(array, message_callback)` consume array values as
alternating keys and values, retaining native DD#99's explicit odd-tail
dispositions. They respectively drop an odd tail, use it as the value of
a fallback key, or raise a requirement failure. An empty array is a valid
empty map; callbacks run only for an odd count. Integer keys stringify,
values keep their type, and later duplicate keys overwrite earlier ones.
This is not an inverse of map value traversal.

`default_value(value, fallback)` and `required_value(value, message)` use
lazy callbacks, shared by scalars and cell reads. Their subject is the typed
value, not membership: `""` or integer `0` triggers the operator, whereas
string `"0"` does not. Requirements raise `CommandError` with status 2.
Key-evaluation, allocation, and conversion failures are not missing values.

Argument construction uses an ordinary owned `std::vector<std::string>`:

| Helper | Contribution |
|---|---|
| `append_argument(args, value)` | One argument, including an empty string or integer zero rendered as `"0"` |
| `append_elided(args, value)` | Zero arguments for `""` or integer `0`; otherwise one |
| `append_splat(args, collection)` | Sorted present values, including empty/zero cells; zero for an empty collection |
| `append_splat_or(args, collection, fallback)` | Present values, or exactly one lazy fallback argument, even if empty |
| `append_splat_required(args, collection, message)` | Present values, or a lazy requirement failure on an empty collection |

Splat preparation publishes no partial extension on allocation failure.
Already-built arguments are independent of later source mutation; scalar
appends are also safe when their source aliases the destination vector.
Binary bytes are retained internally, but the existing exec boundary
rejects NUL in executable/argument strings before any launch.
There is no word splitting or inherited unchecked function mode.
Program ingress and numeric conversion have separate helpers below; the
remaining formatting and source-to-C++ emission are still future work.

### Checked integer arithmetic

Integers remain ordinary `std::int64_t` values. Use `add`, `sub`, `mul`,
`neg`, `div`, `mod`, `pow`, `shl`, and `shr` for operations whose source
semantics cannot be expressed by unchecked C++ operators:

```cpp
std::int64_t quote = sash::mul(sash::add(total, 1), 3);
```

The helpers retain [native DD#25](../notes/decisions/DD25-overflow-traps.md):
addition, subtraction, multiplication, negation, and power overflow fail;
division and remainder reject zero; `INT64_MIN / -1` fails but
`INT64_MIN % -1` returns zero. Division truncates toward zero and remainder
keeps the dividend's sign.

Shifts reject negative counts and never mask them. A left count of 64 or
more returns zero; a right count of 64 or more returns zero or minus one
according to the value's sign. Right shift fills the sign, and left shift
discards bits without treating that as arithmetic overflow. Bounds checks
and unsigned-bit operations keep this standard C++17, without raw signed
overflow or implementation-defined signed shifts.

`pow` uses checked multiplication and exponentiation by squaring, without
an overflowing unused final square. Exponent zero returns one, including
`0 ** 0`; negative exponents retain sash's documented result of zero.

Failures throw `ArithmeticError`, carrying an `ArithmeticFault` and a
fixed diagnostic without an allocating string payload. It is not a
`CommandError` or a returned command outcome. Ignoring an integer result
does not suppress evaluation errors. Main and checked EXIT report the
diagnostic and attempt ERR with status 2 after unwinding. A second fault
in ERR reports and terminates without re-entry.

Comparisons, bitwise operators, `&&`, `||`, and `?:` remain ordinary C++
where their semantics fit. Keep fallible operands inside their lazy
regions; ordinary function arguments do not impose source left-to-right
order, so operands with observable effects or failures may need explicit
temporaries.
There is no checked-number wrapper, operator-overload layer, or IR consumer.
Source diagnostics such as SB5077 and SB5084
remain the producer's responsibility.

### Program inputs and strict integer conversion

Program inputs use explicit C `argc`/`argv`, not hidden global state:

| Helper | Result |
|---|---|
| `argv(count, arguments, start = 0)` | Owned dense string array, including argv0 by default; slices reindex from zero |
| `arg(count, arguments, index)` | Owned argument string; absence raises `SetupError` |
| `arg(count, arguments, index, fallback)` | Eager fallback only for absence, not a present empty argument |
| `getenv(name)` | Owned optional string: absent is `nullopt`, present-empty is an engaged empty string |

Use the ordinary host `argc` for the count, including the program name.
Negative indices and malformed metadata raise `SetupError`; slices starting
at or beyond the count are empty. A low-level empty C++ frame `(0, nullptr)`
is valid, although a real sash program includes argv0. Callers must provide
an actual C argument vector of the stated extent and lifetime; metadata
guards cannot validate arbitrary bad pointer extents.

Snapshots and lookups copy their values. Mutating a snapshot does not mutate
the C frame, and later environment changes do not change a lookup's result.
`getenv` reads the live host environment under the single-threaded model,
without mirroring source variables. It honors counted names, rejects NUL
instead of truncating a name, and never converts allocation failure into
absence. **An unset environment name is an ordinary lookup result, not an
automatic abort** (native DD#101); set-empty remains present.

`parse_int(text)` returns `optional<int64_t>`. One allocation-free,
locale-independent parser also implements `is_int`, `int_or`, and
`int_required` for scalar strings and string cells:

```cpp
auto quantity = sash::int_or(sash::arg(count, arguments, 1, "08"), 3);
auto factor = sash::int_required(sash::cell(options, "factor"), [] {
    return "factor must be an integer";
});
auto amount = sash::mul(quantity, factor);
```

The grammar is native DD#72/DD#77: a complete optional sign followed by
decimal digits or `0x`/`0o`/`0b` digits, with case-insensitive prefixes.
Runtime leading zeros are decimal (`"08"` is eight). Whitespace, partial
parses, missing digits, arbitrary `base#`, separators, NUL, and values
outside signed i64 are invalid; `INT64_MIN` is valid.

`int_or(text, fallback)` defaults on invalidity, including empty or
out-of-range input. **Valid `"0"` returns zero**, not the fallback.
`int_required(text, message)` calls its message exactly once on invalidity
and uses the existing `required value: ` renderer and `CommandError(2)`;
it does not evaluate the message on success. Invalid conversion is a
disposition concern, not `ArithmeticError`. In word context, ordinary
`default_value` still triggers on emptiness, not numeric invalidity.

Failed required conversion preserves the assignment target and follows
ordinary unwinding and ERR routing from main or checked EXIT. ERR must
explicitly handle this operational failure. Source taint, default admission,
and the `i?` predicate's source syntax remain producer responsibilities. There is no
IR consumer or bulk-read family in this unit.

### Counted byte strings and ASCII operations

Ordinary `std::string` values and counted `std::string_view` operands share
these operations for scalars and owned string-cell reads:

| Helper | Result |
|---|---|
| `strlen(text)` | Checked signed-i64 byte count, not a C-string or character count |
| `substring(text, offset, length = nullopt)` | Owned, terminated byte slice with signed/clamped bounds |
| `toupper(text)` / `tolower(text)` | Owned copy mapping only ASCII letters |
| `is_print(text)` | Nonempty, all bytes in `0x20..0x7e` (`p?`) |
| `is_graph(text)` | Nonempty, all bytes in `0x21..0x7e` (`g?`) |

Positive offsets clamp to the end; negative offsets count from the end and
clamp to zero. Omitted length takes the remainder, explicit zero is empty,
positive length clamps to the remainder, and negative length trims bytes
from its end. Bounds include both signed-i64 extremes without negating the
minimum. Empty/default views are valid, and slices can split UTF-8 bytes.

Interior NUL and non-ASCII bytes remain counted data. Case conversion
preserves every byte outside ASCII letters; the predicates reject NUL,
controls, DEL, high bytes, and empty input. Predicate false is an ordinary
answer. No operation uses the host locale or normalizes Unicode.
Length and predicates allocate nothing; slices/case results own their bytes
and propagate allocation failure rather than returning an empty fallback.
No helper adds a hidden checkpoint.

The tenth pair's handling is application-specific: `COUNT=08` becomes key
`count` and unchanged value `08`, then strict numeric conversion yields 8.
There is no trimming or malformed-number repair. Bulk reads/splitting,
remaining formatting, and IR consumption remain separate
work. The required-value failure path retains DD#9's C++ status 2; the
reference currently returns 1 for the same rejected numeric text.

### Regex matching and owned extraction

Compile ordinary `std::regex` values with `compile_regex(pattern)`, which
selects the C++ standard library's modified ECMAScript grammar. Matching
is unanchored unless the pattern supplies anchors:

| Helper | Result |
|---|---|
| `match(subject, regex)` | Boolean search result; ordinary no-match is false |
| `match_text(subject, regex)` | Owned whole match, or empty on no match |
| `match(subject, regex, target)` | Boolean result; dense array `[whole, g1..gN]` |
| `matchall(subject, regex, target)` | Boolean result; flattened groups per match, or whole matches when there are no groups |

Every capture owns its bytes. Nonparticipating and participating-empty
groups retain their empty slots; every group reported by the selected
engine is retained. A successful empty match is true even when its scalar
value is empty. Scalars and string-cell reads share the helpers.

Output arrays are replaced atomically, not appended to. A completed miss
replaces the previous target with empty; matching/copying failure preserves
the previous target. Building locally and swapping on completion is the
simplest C++ implementation and also handles an input view borrowed from
that target without an extra subject snapshot. False is a value, not an
automatic exception; source statement-level checking belongs at the future
emitter's call site. No helper adds checkpoints.

**Target differences are deliberate, not fixes to native sash.**
[DD#11](../notes-exp/decisions/DD11-regex-matching-and-extraction.md) separates
the pragmatic engine/iteration choices from their observable policies.

Subjects use their full counted range, including interior NUL. For example,
`std::string("a\0b", 3)` matched against `b` succeeds here and fails in native
sash, whose Plan 9 subject boundary deliberately stops at NUL. Matching `.`
repeatedly extracts three one-byte values here, including NUL; native
extracts only `a`. This target keeps its strings byte-transparent.

`matchall` uses standard `std::regex_iterator`, not repeated fresh suffix
searches. The impact is visible even with a pattern both engines accept:

| Subject / pattern | This target | Native suffix iteration |
|---|---|---|
| `ab` / `^` | One empty span at the original beginning | Three empty spans, as each suffix becomes a beginning |

Standard iteration also tries a nonempty match at the same position after
an empty match. Under the same ECMAScript engine, `a` / `(|a)` yields
`["", "a", ""]`; an immediate one-byte skip would yield `["", ""]`.
The latter comparison isolates the iteration policy, not Plan 9's different
alternative ordering. End-of-input empty matches are emitted only once.

Invalid patterns and standard regex errors become catchable
`CommandError(2)` with a `regex: ` diagnostic. Allocation failures escape.
Native dynamic `regcomp9` failure instead aborts status 2: an `if`/`||`
does not resume in that process. C++ catching is its existing exception model,
not a claim that native offers equivalent continuation.

Literal NUL bytes in a counted **pattern** are rejected explicitly:
`compile_regex(std::string_view("a\0b", 3))` throws and directs callers to
the ordinary regex escape, `compile_regex(R"(a\x00b)")`. A direct
standard-library probe on this host accepted the literal-NUL pattern but
failed to match it; the escape worked. This small portability guard avoids
silent wrong results without a pattern-rewriting adapter. It does not
truncate or reject NUL-bearing subjects. DG requested a
[native pattern/source backlog item](../notes/tasks/reject-literal-nul-patterns-and-source.md)
separately.

Use compiled regexes from the helper for its admission/error policy; a
caller-supplied precompiled regex no longer exposes its original pattern.
No linear-time or resource-bound guarantee is made for `std::regex`.
Bulk reads/splitting and IR consumption remain separate work.

### Regex replacement

[DD#12](../notes-exp/decisions/DD12-regex-replacement-templates.md) records
DG's selection of standard C++ replacement notation:

| Helper | Result |
|---|---|
| `sub(subject, regex, replacement)` | Owned subject with only its first match replaced |
| `gsub(subject, regex, replacement)` | Owned subject with all standard-iteration matches replaced |

Both preserve unmatched input; a normal miss returns an owned unchanged
subject. Empty replacements delete matches. Empty matches retain DD#11's
standard progression, including the final boundary and nonempty retry;
`sub` stops after its first match, even when empty.

Use `$&` for the whole match, `$1` through `$99` for numbered groups, `$$`
for a literal dollar, and the standard per-match prefix/suffix expansions.
Backslashes are ordinary bytes: `\1` is not a group reference here.
Computed templates follow the same format rules as literals; no native
backslash validator or runtime rewriting adapter is added.

Subjects and templates are fully counted through NUL/high bytes, including
partial views. Standard iterator matches feed `match_results::format` with
explicit begin/end pointers: the convenience `regex_replace` string-format
overload truncated a NUL-bearing template on this host. The formatter
borrows its template; results own their bytes without a subject/format
snapshot. Scalars and string-cell reads share the helpers.

Regex errors retain the existing status-2 exception; allocation/length
failures escape rather than return partial, empty, or unchanged success.
Caller assignment happens only after successful evaluation, including
destination-borrowed subject/template views. There are no hidden checkpoints
or hidden lifecycle guards.

The future IR consumer must translate native `\0`/`\1` notation, including
evaluated templates and literal dollars/backslashes. That is separate from
C++ source-string escaping and is not implemented by this runtime unit.

### Program lifecycle, EXIT, and ERR

[DD#15](../notes-exp/decisions/DD15-checked-exit-explicit-err.md) adopts
implemented native DD128, replacing the former END contract without an alias.
`run(program, body[, on_exit[, on_error]])` borrows separate optional
callbacks; pass `nullptr` for an absent slot. Metadata overloads take the
source-fd set before `body`. A body returns void for zero completion or
exactly `std::int64_t` for its full fall-through status.

Handlers return void and receive `const Shutdown &`. Its reason is
`Completed`, `ExplicitExit`, `Failure`, or `Signal`; its full-i64 status
and triggering signal remain separate. Only `run`'s process-boundary
return reduces status to its low byte.

| Cause | Selected handler |
|---|---|
| Normal fall-through with any saved status | EXIT |
| Explicit full-i64 zero, including argumentless exit | EXIT |
| Cooperative EXIT signal dispatched at a checkpoint | EXIT |
| Explicit full-i64 nonzero exit | ERR |
| Uncaught operational failure or first fatal arithmetic/heap fault | ERR |

`exit_program(status = 0)` raises a dedicated `ExitRequest`, not a command
verdict or function return. Selection uses cause and the complete integer,
not the process byte: `exit_program(256)` enters ERR with 256, not EXIT.
An absent handler never substitutes the other one.

EXIT is checked like main, including called functions, operands, and setup.
Uncaught failure or explicit nonzero exit in EXIT enters ERR once; explicit
zero terminates. Explicit exit in ERR terminates without entering EXIT.
Successful callback completion preserves its incoming status.
First fatal arithmetic/heap failure in main or EXIT also attempts ERR;
a fault in ERR reports and terminates without another entry or allocation.

ERR's source operations need explicit handling; discarding a function's
returned value does not suppress its body. An unhandled operational
exception in a hand-written host ERR callback escapes the C++ caller,
rather than receiving implicit catch-and-continue. Future source admission
must enforce explicit handling, conservative incoming-taint use through
`:!`, and the distinct adjacent opportunity for genuinely fresh ERR input.
There is no temporary validator or runtime taint machinery.

Source function definitions are directly top-level only, never in either
handler or another nested scope. Generated handler functions read actual
process-global storage, not a hypothetical final-state snapshot.
Default-initialize that backing storage and execute source initializers
inside the body. Completed writes survive; interrupted assignments do not
publish. The terminal pair demonstrates this across an EXIT capture.

C++ unwinding restores captures and closes pipeline endpoints before
reaping and callback entry. Invoker stdio is restored before each handler,
including EXIT -> ERR. Entry itself allocates nothing; source reservations
remain valid through heap failure during channel bootstrap. Ordinary
channel-setup failure still runs neither body nor handler, and failed
restoration prevents invoking a callback on known-wrong channels.

The lifecycle checks its owning PID before handlers and driver return.
External pre-exec children never unwind inherited C++ objects; explicit
program exit in a forked child uses `_exit`, not parent handlers.
This does not solve future forked-C++/job inherited-reader ownership.
Broken descriptor, reap, signal-restoration, or lifecycle-handoff guarantees
retain the existing nonallocating fatal-cleanup policy.

### Signals and cooperative checkpoints

Within `run`, the owner calls `signal(number, SignalDisposition::Ignore)`,
`Default`, `Flag`, or `Exit`. Registration failures are `SetupError`;
allocation may also fail before the kernel disposition changes.
Original dispositions and the incoming mask are restored when the driver
leaves, including after an escaping host ERR exception.

The two signal handlers perform only lock-free integer stores.
`signum()` reads the most recently delivered handled signal;
`clear_signum()` clears that information, not a pending EXIT request.
FLAG cannot become EXIT merely because another signal declares EXIT or
the same signal is later rearmed. `Shutdown::signal` retains the pending
EXIT identity even if a later FLAG changes `signum()`.

`checkpoint(bool deferred = false)` raises a signal exit request in ordinary
execution context only when dispatch is allowed. A true argument defers
dispatch, not receipt or `signum()` updates. Neither phase of terminal
handling can redispatch; ordinary failure and explicit exit keep their
own routes even while a signal is pending.

Polls belong before/after relevant commands, pipelines, functions, and
loops, including return and reader false/completion edges. The future
consumer must derive deferral through acquisition and its legal adjacent
attest or direct failed-branch replacement, not simply poll after every
statement. Command, reader, and capture APIs add no hidden checkpoints.
`run` has fixed body-entry and normal-completion checks; it installs no
EXIT dispositions automatically. Signal-free generated code should add
no polls. The [future regression task](../notes-exp/tasks/ir-signal-checkpoint-emission.md)
pins both conditional emission and protected placement once a consumer exists.

Blocking reads and waits retry EINTR. Signal recording does not interrupt
ownership cleanup or automatically kill children: shutdown waits for the
next checkpoint, which can be delayed by foreground work or pipe EOF.
The fixtures use finite helpers and a two-way pipeline handshake.
Linux traces confirm interrupted reads/waits before ordinary-context shutdown;
the same behavior fixtures run on the real Mac using host signal constants.
There is still no runtime timeout or force-kill policy.

### Command-backed line readers

The third pair uses the checked form:

```cpp
sash::read_from("/usr/bin/printf", {"Ada\nGrace\n"}, [&](sash::LineReader &input) {
    std::string person;
    while (input.readline(person)) {
        sash::echo({person});
    }
});
```

The callback runs in the parent, so mutations to captured variables and
reference parameters remain visible. Only the producer child's stdout
is connected to the pipe. Parent stdin and stdout are not rebound: an
ordinary command in the loop still inherits program stdin, and captures
or nested readers have their own destinations and ownership.

`readline` removes one newline delimiter, retaining all other bytes,
including NUL and carriage return. An empty terminated line succeeds;
an unterminated final nonempty record succeeds once. EOF clears the
target and returns false. A read error raises `SetupError`, not EOF,
and leaves the target unchanged. These are the DD#99/DD#122 reference
rules, not whitespace splitting or a new interpretation of `read`.

`try_read_from` returns the producer's `ProcessResult`; `read_from`
checks it. Setup failure before the consumer starts does not invoke it.
Consumer exceptions propagate as themselves, not as producer outcomes.
Already-consumed records and parent mutations are not rolled back if
the producer subsequently fails.

Completion closes the reader before waiting, including when a callback
returns early or throws. Early closure can give the producer SIGPIPE;
the unchecked form retains that signal outcome. During unwinding the
consumer's original exception survives successful cleanup. There is no
force-kill policy or runtime timeout: waiting can still block if a
producer ignores the closed pipe and keeps running.

The `LineReader` is borrowed only for the callback's lifetime; a `return`
in the callback ends that region, not its surrounding C++ function.
The same explicit reader is also used by the multi-stage API below.

### Owned NUL record reads

`read(std::string_view)` splits a counted string; `read(int fd = 0)`
consumes a borrowed descriptor through EOF, defaulting to stdin. Both
return an owned `Array<std::string>` with dense zero-based indices.
String-cell values use the same operation. For command/pipeline callbacks,
use the existing reader's `read()` so prefetched line-reader bytes survive:

```cpp
auto fields = sash::read(std::string_view("a\0\0b\0", 5));
sash::read_from("/usr/bin/printf", {"%s\\0", "Ada", "Grace"},
                [&](sash::LineReader &input) { fields = input.read(); });
fields = sash::read(std::string_view{});
```

Only NUL terminates records. Leading/interior empty records survive,
a final NUL adds no synthetic field, and a nonempty unterminated tail
survives. Newline, CR, whitespace, and high bytes remain ordinary data.
Empty input/EOF returns an empty array; assigning it clears old cells.
Any other separator is converted upstream, not supplied as a read flag.
[DD#13](../notes-exp/decisions/DD13-nul-record-reads.md) records the contract.

The raw-fd helper never closes or reflags its descriptor. The buffered
method gathers remaining bytes locally and shares the string splitter,
temporarily holding both the full input and owned fields. I/O failures
raise a labeled `SetupError`, not EOF; allocation/length failures escape.
Caller assignment remains unchanged on failure, including a source borrowed
from that target. Already-consumed bytes are not rewound, and producer
failure after completed assignment does not roll it back. Existing
command/pipeline owners still close before waiting during unwinding.
No read helper adds checkpoints, cancellation, taint checks, or lifecycle policy.

### External pipelines

The checked form takes an ordered vector of commands:

```cpp
sash::read_pipeline(
    {{"/usr/bin/printf", {"Grace\nAda\nGrace\n"}},
     {"/usr/bin/env", {"LC_ALL=C", "/usr/bin/sort"}},
     {"/usr/bin/uniq", {}}},
    [&](sash::LineReader &input) {
        std::string person;
        while (input.readline(person)) {
            sash::echo({person});
        }
    });
```

`try_read_pipeline` instead returns a `PipelineResult`. Its `stages` vector
has one optional `ProcessResult` per command; empty slots mean unstarted
stages, never invented exit zero. Observed pre-exec failures stay setup
outcomes rather than ordinary exit 127. `setup_errors` separately records
setup/cleanup errors with zero-based stage indices, in encounter order,
without overwriting known child exits or signals.

An empty command list is an invalid API request and raises `SetupError`,
including in the inspectable form. Allocation failures and consumer
exceptions may also escape rather than becoming stage outcomes.

Success requires every stage to succeed and no setup error. Checking first
raises the earliest setup error, otherwise the rightmost failed stage's
exit/signal. All started children are accounted for before ordinary
verdicts are checked; consumed parent state is not rolled back.
[DD#3](../notes-exp/decisions/DD03-external-pipeline-outcomes.md) records
DG's choice.

Command strings are checked before any fork. All data pipes are allocated
before launch, and each later child's stdin comes from the preceding pipe.
Parent stdin is never rebound. Setup failure skips the consumer, closes all
remaining parent endpoints before waiting, and leaves unstarted stages
explicit. Each endpoint is attempted even if another close fails.
Consumer exceptions propagate unchanged after successful cleanup.

This slice is external stages plus a terminal parent reader, not forked
C++ stages, public wait, stage suppression, negation, or persistent redirects.

### Raw external-pipeline capture

The fifth pair uses the checked byte collector:

```cpp
const auto text = sash::capture_pipeline(
    {{"/usr/bin/printf", {"Ada\nGrace\n"}}, {"/bin/cat", {}}});
```

`try_capture_pipeline` instead returns `PipelineCapture`, with a counted
`output` string and a separate `PipelineResult` in `pipeline`. Failed
producer exits and completion errors do not erase already collected bytes.
The checked wrapper checks the pipeline before returning the string, so a
failed right-hand capture does not replace an existing assignment target.

The parent drains the terminal pipe concurrently into the string, preserving
NUL, CR, separators, and every final newline. It does not use capture storage or
rebind parent stdin/stdout; it shares preparation, launch, endpoint ownership,
and completion with the line-reader pipeline. The terminal command observes
a FIFO on stdout. Raw capture can run inside an in-process capture body or a parent-reader
loop without adopting their channels.

Command validation and setup failure before consumption leave output empty
and retain unstarted stages. An empty command list throws `SetupError`.
Consumer read errors, allocation failures, and string-capacity errors also
escape the inspectable form: they are not EOF or invented child outcomes,
and no partial `PipelineCapture` is returned. Successful cleanup closes all
parent endpoints before reaping; a cleanup failure retains the existing
fatal policy.

Completion means pipe EOF followed by accounting for every owned stage.
A descendant retaining the output pipe can delay EOF; there is no runtime
timeout or automatic force-kill. Background-writer language semantics remain
separate work. Binary capture does not permit NUL in OS argument strings.

### In-process capture and remaining boundaries

`capture_stdout` redirects actual fd 1 to owned seekable storage and runs
its void body in-process. Nested frames and exception unwinding restore
the previous destination, including an originally closed stdout and its
descriptor flags. This low-level operation returns exact bytes, including
NUL and trailing newlines; source-level newline treatment remains an open
choice. The examples use newline-free `write` or `printf` inside capture,
so that choice does not affect their equivalence.

Linux uses memfd without a filesystem fallback. Darwin uses a private
mkstemp file, unlinked before the body writes and marked close-on-exec.
Temporary-directory, creation, unlink, and flag failures are explicit
setup errors; a failed setup never executes the body. Raw external capture
still uses a pipe and succeeds even if TMPDIR is invalid.
Darwin shared-memory fds cannot provide ordinary read/write operations.
See [DD#4](../notes-exp/decisions/DD04-macos-posix-adapter.md).

The runtime is initially single-threaded and its output helpers use raw
descriptor writes. Captured code must not mix in buffered stdio/iostream
output or retain background writers. Temporary redirections and source-fd
reservation are implemented above; persistent redirects and further
pipeline forms remain later work.
Failure to restore descriptor state during unwinding is diagnosed
and terminates rather than continuing on the wrong stdout.

Shared POSIX operations and ownership live in `src/platform_posix.cpp`.
`src/lifecycle.cpp` owns the shared POSIX signal and program boundary.
`src/platform_linux.cpp` supplies memfd, pipe2, and environment access;
`src/platform_darwin.cpp` supplies unlinked capture files, pipe/CLOEXEC,
and CRT environment access. Public interfaces and examples have no
Linux-only headers. The build chooses the host adapter and recreates its
archive so a stale backend member cannot remain active.

Project [status](../notes-exp/status.md),
[tasks](../notes-exp/tasks/cpp-transpiler-runtime-before-ir.md),
[decisions](../notes-exp/decisions/DD15-checked-exit-explicit-err.md), and
[journal](../notes-exp/journal/022-checked-exit-explicit-err.md) live in
`../notes-exp/`, not the native compiler's `notes/`.
