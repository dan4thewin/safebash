# safebash — A Guide to the Architecture

*For anyone continuing or contributing to this project.*

This document explains how safebash works and why it is designed the
way it is. It is written for someone with a working knowledge of
compilers and Rust, but not necessarily deep expertise in either.

**A note on authorship:** This guide was written by an AI (Claude,
Anthropic) working under the direction of DG, the human creator of
safebash. The AI wrote the prose and the code; DG directed the
effort and made design decisions. See `notes/journal/001-prologue.md`
for a fuller account of how this collaboration works.

---

## The Chain of Trust

safebash exists to make shell scripts participate in a chain of trust:

```
Author writes safe-subset script
    → sash validates and compiles to native binary
        → Binary is signed in the build pipeline
            → System policy (IPE / fs-verity) verifies before execution
```

Every architectural decision serves this chain.

## Safety Model

Three dimensions:

1. **Language-level safety.** The safe subset eliminates constructs
   that make Bash difficult to analyze: no eval, no indirect expansion,
   no implicit coercions. The compiler handles bash idioms silently
   (backticks → `$()`, `[ ]` → `[[ ]]`, unquoted `$var` is safe because
   sash has no word splitting or globbing).

2. **Data-level safety.** Taint quarantine (DD#24, DD#42–44). All
   externally-sourced data (env vars, positional params, read input,
   external command output) is tainted. Tainted data cannot be used
   at all until validated via `attest VAR EXPR`. Environment variables
   are accessed via `$(getenv NAME)`, positional params via `$(argv N)`.
   No "variables just appearing" — every value has a visible origin.
   `$(getenv NAME)` also reports a *lookup* status (DD#101) — success when
   the name is present, including when it is set to the empty string;
   failure when it is unset — so `home=$(getenv HOME) || home=/root`
   distinguishes absent from empty. It is the one intrinsic admitted to
   the DD#88 capture-condition; the rest are pure functions of their
   arguments and cannot fail.

3. **Supply-chain safety.** Compiled artifacts are signed. The system
   refuses to run binaries whose signature or integrity check fails.

Safety does NOT mean sandboxing. External commands (`rm`, `mount`,
etc.) are legitimate operations. Safety means the script's intent is
visible, its data boundaries are explicit, and its binary is
tamper-evident.

---

## Two-Tool Model (DD#57)

The toolchain splits concerns between two binaries:

- **sash** — the compiler. Accepts only sash-native syntax. Validates,
  type-checks, and compiles to static ELF. Never transforms source.
- **sash-convert** — the converter. Accepts Bash and produces sash-native
  source with mechanical coercions, MANUAL annotations for constructs
  needing human review, and CHECK/ATTEST hints.

The split enforces a clean boundary: sash-convert is a best-effort
transformation tool; sash is a sound compiler. A script that compiles
with sash is safe by construction. A script that only passes through
sash-convert still needs human review.

Typical workflow:
```
bash script.bash → sash-convert → script.sh (review) → sash → binary
```

---

## Repository Structure

```
safebash/
├── sash/                  — The compiler (Rust crate)
│   ├── src/
│   │   ├── main.rs        — 4-line wrapper calling sash::cli_main()
│   │   ├── lib.rs         — CLI entry, pipeline driver
│   │   ├── parse/         — hand-written lexer + recursive descent → AST
│   │   │   ├── logos_tokens.rs — the logos `Tok` enum
│   │   │   ├── lexer.rs   — Scanner (peek/peek_n/next)
│   │   │   ├── parser.rs  — recursive descent → Program
│   │   │   ├── expr.rs    — words, strings, expansions, subscripts
│   │   │   ├── arith.rs   — `(( ))` / `$(( ))`, DD#58 sexpr calls
│   │   │   └── error.rs   — parse_error helpers
│   │   ├── ir.rs          — IR definitions (the AST the parser yields)
│   │   ├── validate.rs    — structural rules + taint + positionals + splat
│   │   ├── type_check.rs  — Type checking (int vs string)
│   │   ├── resolve.rs     — DD#23 command resolution
│   │   ├── transform/     — IR → SSA (entry, statements, control_flow,
│   │   │                    expressions, functions, pipelines, handlers)
│   │   ├── ssa.rs         — SSA representation and printing
│   │   ├── ssa_taint.rs   — SSA-level taint verification
│   │   ├── codegen/       — Cranelift code generation (entry, values,
│   │   │                    commands, arithmetic, dispatch, redirects,
│   │   │                    setup, types, handlers)
│   │   ├── emit_c.rs      — C oracle backend
│   │   ├── emit_ir.rs     — IR capture (inert Janet data)
│   │   ├── regex_validate.rs — DD#39/#63 regex validation
│   │   └── diagnostic.rs  — Error reporting
├── sash-convert/          — Bash → sash-native converter
│   └── src/
│       ├── main.rs        — CLI entry point
│       ├── convert.rs     — tree-sitter-bash CST → sash-native source
│       └── known_externals.rs — Auto-attest patterns
├── sash-rt/               — Runtime library (#![no_std], rustix, no libc).
│                            A SEPARATE cargo workspace — build it with
│                            `(cd sash-rt && cargo build --release)`.
├── sash-expand/           — Expansion utilities
├── docs/                  — GUIDE, TIMELINE, sash.1
├── notes/                 — iwe knowledge graph (journal, decisions, tasks)
├── audit/                 — Adversarial review results
│   └── run-1/             — 5 models, 3 passes, remediation done
├── spec/                  — Spec test suite (41 files; results in spec.db)
├── contrib/               — Vendored: plan9port-regexp, freertos-heap4
├── suites/                — brush/oils test runners
├── exp/                   — Git submodule (safebash-exp, optional)
└── mkfile                 — Plan 9 mk build system (build, test, clean)
```

**The compiler has no tree-sitter dependency.** `sash-convert` uses *stock*
`tree-sitter-bash 0.25`, and it uses it on *bash* input — the language it
reads, not the language it writes. The old `~/src/tree-sitter-bash-safebash`
fork drove a pipeline that commit `1c0e64a` deleted, and is no longer part
of the build.

---

## Compilation Pipeline

```
 Bash source (.sh)
       │
  [1]  │  Parse
       ▼
 parse/ (logos lexer + recursive descent) → Program (AST)
       │
  [2]  │  Validate
       ▼
 validate.rs → diagnostics (SB4001–SB50xx)
       │
  [3]  │  Taint
       ▼
 validate.rs::check_taint → per-definition taint (SB4020/SB5020)
       │
  [4]  │  Lower
       ▼
 parse/ already yielded the IR (ir.rs); coercions are applied
 by the parser and the transform — there is no separate lowering pass
       │
  [5]  │  Type check
       ▼
 type_check.rs → int vs string inference, declare -i stickiness
       │
  [6]  │  Transform
       ▼
 transform/ → SSA (ssa.rs): versioning, phi nodes, basic blocks, CFG
       │
  [7]  │  Fold
       ▼
 Constant folding in compile_to_object
       │
  [8]  │  Codegen
       ▼
 codegen/ → Cranelift IR → object code (.o)
       │     Declares extern functions from sash-rt
       │     Embeds .sash_source/.sash_ir/.sash_manifest
       │
  [9]  │  Link
       ▼
 lib.rs invokes ld → object + libsash_rt.a → static ELF binary
```

### Phase 1: Parse

A hand-written lexer (`parse/logos_tokens.rs` + `parse/lexer.rs`) and
recursive-descent parser (`parse/parser.rs`) produce the AST directly.
There is no CST and no tree-sitter dependency in the compiler — commit
`1c0e64a` removed both. (`sash-convert` still uses stock
`tree-sitter-bash` on *bash* input; that is a separate crate.)

### Phase 2: Validate

`validate.rs` walks the AST once. (The old `rules/` directory and its
`Rule` trait are gone — commit `1c0e64a` folded them, the standalone
taint pass, and the CST itself into this single module.)

| Check | Codes | What it catches |
|-------|-------|-----------------|
| `check_dynexec` | SB4001 | `eval`, `source`, `.`, `command`, `coproc` |
| `check_heredoc` | SB5002 | heredocs / here-strings |
| `check_command_head` | SB5011 | an unknown name in command position |
| `normalize_command_head` | SB5096, SB4074 | a quoted command name that has a bare spelling, or an empty one (both parse time) |
| `check_exec_target` | SB5011, SB5095 | an `exec` target that is not a runnable program |
| `check_taint` | SB5020 | a tainted value used without `attest` |

There are **no reserved words.** A bash builtin sash does not implement
(`export`, `declare`, `typeset`, `readonly`, `select`, `shift`, …) is
simply an unknown command and gets SB5011 like any other unresolvable
name — so any of them can be defined as a user function and called.
(A parser table used to intercept six of them with bespoke diagnostics;
DG retired it on 2026-09-12 because it both reserved the names and made
the unknown-command story non-uniform.) `eval` / `source` / `.` are the
exception: they are *rejected*, not merely unknown, and cannot be
redefined away.

### Phase 3: Taint

Flow-sensitive taint analysis (DD#24, DD#42–44), tracking each variable
as `Tainted`, `Attested`, or `Clean`.

**Two phases (DD#100).** Phase 1 walks the top level, deferring every
function definition; phase 2 then checks the deferred bodies against the
completed set. A single sequential walk checked each body *at its
definition*, so a global tainted later was invisible inside it — `fn user
{ for v in @g; … }` followed by `argv g` compiled and leaked. The two
phases need no fixpoint because **SB5067** forbids a function from
tainting a global (use a local, attest it, assign out), which makes the
top level the only place a global's taint can arrive.

**Collection slots (DD#100)** are a call-site fact, so no phase ordering
exposes them. `analyze_slots` resolves both directions ahead of the walk
with a call-graph fixpoint: inward, a slot is tainted if *any* call site
passes a tainted collection; outward, a callee that bulk-fills a slot
taints the caller's collection.

**Taint sources:** `$(argv N)` (program arguments), `read`
targets, `$(external cmd)`, environment variables via `$(getenv NAME)`.
Function-local `$1`–`$N`/`$@` are always clean — passing a tainted
value as a function argument is a read, which is a compile error.
Top-level `$1`–`$N` are compile errors (DD#46: use `$(argv N)`).

**Clean sources:** literals, arithmetic from clean operands, `$0`,
`$#`, `$(status)`, `$(getpid)`, `$(getcpid)`, `$(waitstatus)`, variables assigned from
clean expressions. The punctuation spellings `$?`/`$$`/`$!` are input
sugar for the last three (DD#75); the keyword form is canonical and is
what `sash-convert` emits. As an arithmetic operand the spelling is the
`(name)` sexpr — `$(( (status) + 1 ))` — not the command substitution,
which is SB4020 there.

**Auto-attest:** `known_externals.rs` provides auto-attest patterns
for 26 environment variables and 25 commands.

**`attest VAR EXPR`** — evaluates EXPR (tainted VAR readable inside
the expression). If truthy, marks VAR clean. If falsy:
- Bare `attest` → fatal exit
- `if attest` → false branch taken

```bash
port=$(argv 1) || { echo "usage: $0 port"; exit 1; }
attest port [[ $port =~ ^[0-9]+$ ]]
echo $port   # clean — attested above
```

**The rule:** you can write tainted data into a variable but cannot
read it except inside `attest`. Diagnostic: SB4020.

**Containers (DD#61):** a tainted value cannot enter an array/map via a
literal, `+=` append, or `a[i]=` subscript (SB5021); tainted arrays come
only from bulk reads (`read -a`, `mapfile`). Collections are attested
per-element with attest-all — `attest x in a [[ … ]]` (indexed, binds each
value) or `attest k v in m [[ … ]]` (map, binds key and value). A failing
element aborts; an empty collection passes vacuously. Scalar `attest` on
a collection is a type error.

### Phase 4: Lower — no longer a phase

There is no lowering pass. The recursive-descent parser produces `ir.rs`
types directly, so the CST→IR step commit `1c0e64a` removed had nothing
left to do.

The coercions that pass used to perform now happen in two places:

- **In the parser**, where a surface spelling has a single IR form —
  `declare` → typed declarations (int, array, map), `let` → `(( ))`,
  backticks → `$()`, `[ ]` → `[[ ]]`.
- **In the transform**, where the choice depends on types or on
  resolution — glob→regex for a pattern, int/string conversion, command
  resolution (DD#23, `resolve.rs`).

The step numbering below is kept as-is so that phase names in the journal
and in diagnostics still line up.

### Phase 5: Type Check

`type_check.rs` infers int vs string for each variable. Tracks
`declare -i` stickiness. Cross-type assignment without conversion
is a compile-time error.

- `int x=5` / `declare int x=5` / `declare -i x` — int variables
- `fn name() -> int { }` — int-returning functions
- `$#`, `$(status)` (sugar: `$?`), `$((...))` — intrinsically int

### Phase 6: Transform

`transform/` converts IR → SSA. Variable versioning, phi nodes at
merge points, basic blocks with explicit terminators, control flow
graph construction. Function inlining. Arithmetic evaluation.

#### Functions, inlining, and recursion (DD#27)

Functions are **inlined**. There is no runtime call stack, so a function
cannot in general call itself — inlining would not terminate. `SB5037`
rejects those calls.

**One shape is allowed: a direct self tail-call**, where the recursive call
is the producer of a `return`. That compiles to a **loop**, so it runs in
O(1) stack. Write recursion in accumulator form:

```sash
fn fact -> int {
  int n=$(($1)); int acc=$(($2))
  if (( n <= 1 )); then return $(( acc )); fi
  int n1=$(( n - 1 )); int a1=$(( n * acc ))
  return $(( (fact n1 a1) ))     # tail self-call → compiled to a loop
}
int r=$(( (fact 5 1) ))          # 120
```

Everything else is `SB5037`:

| shape | result |
|---|---|
| direct self tail-call — `return $(( (f …) ))` | compiled to a loop |
| non-tail self-call — `return $(( n * (f m) ))` | **SB5037** |
| mutual recursion — `f` calls `g` calls `f` | **SB5037** |

The non-tail case is the one worth naming explicitly, because it is what a
direct translation of the textbook factorial produces: the multiply happens
*after* the call returns, so there is nothing to turn into a loop.

### Phases 7–9: Fold, Codegen, Link

Constant folding, then `codegen/` lowers SSA to Cranelift IR. Declares
extern functions from `sash-rt`. `codegen/entry.rs::embed_artifacts`
embeds the source, the IR capture and the manifest as `.sash_source`,
`.sash_ir` and `.sash_manifest` ELF note sections (DD#45, DD#109). Finally,
`lib.rs` invokes `ld` to link the object with `libsash_rt.a` into a fully
static ELF binary.

---

## IR (Intermediate Representation)

Defined in `ir.rs`.

### Statement types (StmtKind)

Assignment, Declaration, Command, Pipeline, List (&&/||), If, While,
For, Case (the `[[ ]]` dispatch form, DD#66), Function, Redirected,
Subshell, Group, ArithmeticCommand.

Subshell exists in the IR for bash-mode input; the transform coerces it
into sash primitives (DD#38).

### Expression types (ExprKind)

Literal, SimpleExpansion, DoubleQuoted, SingleQuoted,
CommandSubstitution, ArithmeticExpansion, Concat,
ParameterExpansion, Array, AssocArray, Subscript, Test.

---

## SSA (Static Single Assignment)

Defined in `ssa.rs`.

- **SsaProgram** — functions, each containing `Vec<BasicBlock>`
- **SsaInst** — Assign, Command, Pipeline, Test, ReadLine,
  ReadLineMulti, CaptureStart/End, Export, Attest, Trap, etc.
- **SsaValue** — Literal, Var, ParamExpansion, ArithExpr,
  ArithBinOp, Concat, DoubleQuoted, etc.
- **VarRef** — `{ name, version }`
- **PhiNode** — `{ target, sources: Vec<(BlockId, VarRef)> }`
- **Terminator** — Goto, Branch, Return, Loop

Version state uses a high-water mark counter that never rolls back,
preventing version collisions across if/else arms.

Elidable command substitution: when `$(func)` calls a known function
whose body is all `echo` with literal args, the output becomes a
compile-time constant.

---

## Runtime (sash-rt)

`sash-rt/src/lib.rs` (~3750 lines) — a `#![no_std]` Rust crate.
Uses `rustix` for raw Linux syscalls. No libc dependency.

### Entry point

`_start` — inline assembly for x86_64 and aarch64. Sets up argc,
argv, envp from the stack, calls `sash_main`.

### Memory

`sash_alloc` — bump allocator.

### String operations

`sash_concat`, `sash_itoa`, `sash_atoi`, `sash_strlen`,
`sash_substring`, `sash_strip_prefix`, `sash_strip_suffix`,
`sash_toupper`, `sash_tolower`, `sash_regex_match`,
`sash_regex_match_into`, `sash_regex_replace`, `sash_fnmatch`.

All strings are `(ptr, len)` pairs with a null terminator at
`ptr[len]`. This gives length-bounded operations for safety and
null-terminated C interop for `execve` without copying (DD#12).

### Regex

Plan 9 regex engine, vendored from `contrib/plan9port-regexp`. A
builtin family (`match` / `sub` / `gsub`) takes ordinary string args
(DD#63); the `${var:s/re/str/flags}` / `[[ =~ ]]` syntaxes are thin
sugar. **Captures** (DD#63): `match S P ARR` fills `ARR` with
`[whole, g1..gN]` (bash-parity: `N` = the pattern's group count, surfaced
via the engine's `Reprog.nsub`; a non-participating group is empty),
clears the array on entry, and returns the boolean. `sub`/`gsub`
replacements expand `\0`–`\9` backrefs and `\\`; any other `\<char>` is a
compile error (SB4033).

**Two boundaries are worth knowing, because they are the whole dialect's
sharp edges.** Full treatment in `sash.1` §REGULAR EXPRESSIONS.

The engine is **byte-oriented**, not character-oriented: `.` consumes one
byte, so `é` needs `..`. That is not a design choice so much as a
consequence of `sash-rt/csrc/lib9.h`, whose `chartorune` is an eight-line
ASCII-only stub — the single most user-visible divergence from bash, and
the one least visible in the source.

A regex operation examines its subject only **up to the first interior
NUL**; bytes past it are neither matched nor replaced, and the operation
reports success. Everything else the engine cannot do is *rejected*
(SB5023); this and the unsupported POSIX bracket classes (`[[:alpha:]]`,
read as an ordinary bracket expression) are the only two that fail
quietly. Documented deliberately rather than fixed — see DD#63's addendum
and `notes/tasks/limitation-regex-stops-at-interior-nul.md`.

Counted repetition (`{n}`, `{n,m}`, `{n,}`) caps each bound at **32**;
33 or more is rejected. The cap is a memory-safety bound, not an
arbitrary limit: `regcomp.c` sizes its instruction array from the pattern,
and an unbounded count overran it (journal 290).

### I/O

`sash_echo`, `sash_write`, `sash_read_line`,
`sash_heredoc_redirect`.

### Process management

`sash_exec` — execve with absolute path (no PATH walk; DD#23).

Command substitution has **two** capture paths, chosen per site at compile
time by `transform::capture_purity`:

* `sash_capture_start_buf` / `sash_capture_end_buf` — the fast path. The body
  is *pure*: nothing in it forks onto this capture's stdout and nothing
  rebinds fd 1. Output is collected in a userspace buffer, fd 1 is never
  redirected, and the capture costs **zero syscalls**. `sash_write` diverts
  at the `fd == 1` boundary.
* `sash_capture_start` / `sash_capture_end` — the general path. fd 1 is
  pointed at a **memfd** (not a pipe: a captured body runs in-process with
  nobody draining, so a pipe self-deadlocks past 64 KiB). Used whenever the
  body contains a pipeline, a redirect, or a bare external.

A body may contain an external and still be pure, so long as the external's
output arrives through an *inner* `$( )` — that child gets a pipe of its own.
Only a fork inheriting *this* capture's stdout disqualifies a site.

The runtime keeps a **stack** of capture frames, because an impure capture
can nest inside a pure one: the memfd path pushes a transparent frame that
suspends buffer diversion for its duration. Depth is static: functions are
inlined, and the one recursive shape sash accepts — a direct self tail-call —
becomes a loop rather than a nested frame. Everything else is SB5037. (See
[Functions, inlining, and recursion](#functions-inlining-and-recursion-dd27).)

One consequence is deliberately observable: a pure body probing its own
stdout (`[[ -p /dev/stdout ]]`) sees the caller's real stdout, while
an impure one sees a regular file. Specs `cap.12` and `cap.17` pin both
halves. See journal 277.

### Environment

`sash_getenv`, `sash_setenv`.

### File tests

`sash_file_exists`, `sash_file_is_dir`, `sash_file_readable`, etc.

### Arrays

`SashTable` — hash table implementation used for both indexed and
associative arrays (DD#33).

---

## Backends

### Native (primary)

Cranelift → static ELF binary. No libc, no dynamic linker. Result
is a ~16KB statically linked executable.

### C (the oracle)

`emit_c.rs` — `--emit-c` writes C on stdout instead of an object. The C is
compiled by `gcc` and linked against the same `libsash_rt.a` as the native
backend, so the two share a runtime and differ only in how the SSA reaches
machine code.

This is not a portability story; it is a **differential oracle**. The whole
spec suite runs twice, once per mode (`SASH_MODE=native` and `SASH_MODE=c`),
and a divergence between them localises a bug to one backend without a
reference implementation having to exist. It is the reason a codegen
regression usually announces itself as a single failing spec in one mode
rather than as a wrong answer in both.

### IR capture

`emit_ir.rs` — `--emit-ir` writes the IR as an **inert Janet data
literal**: a value the reader builds, not a program to execute.

This is the decoupling seam (DD#104). The in-tree Janet backend was
removed because it had stopped gating (41 permanently-tolerated
failures) while still taxing every new spec (~28% skipped). A future
oracle is a separate program that reads a capture, so it can lag the
compiler without blocking it.

Two properties are load-bearing: every match is exhaustive with no
`_` arm, so a new IR variant is a compile error rather than a silent
omission; and tags are IR node names, never an interpretation
(`attest` is `{:kind :command :name "attest"}`) — interpretation
belongs to the consumer.

### Manifest

`--manifest` prints the program's external-command manifest: every binary
the program can execve, resolved to an absolute path at compile time
(DD#23). It is what makes a compiled script auditable without running it.

Every path in it is one the compiler confirmed is a regular file with an
execute bit, in the resolution root (`--sysroot`, or the build host). Until
SB5095 that was not true: an absolute head was never looked at, so a
manifest could list a path the compiler already knew was absent — a
requirement that cannot be met, asserted as fact.

A resolution root that cannot be read is a **usage error reported before
compilation starts**, not a language diagnostic. An unreadable image used to
yield an empty listing, so every lookup in it answered "not there" and every
command head in the program reported SB5095 — one bad compiler argument
buried under a diagnostic per line, each blaming the user's source.

There is no globbing in command position. `*` is a glob only in an array
context (DD#39, scoped by DG on 2026-10-10), so `/bin/ech*` is a program
that does not exist rather than a pattern. A command name with no bare
spelling — a path containing a space — is written in single quotes, which
denote the literal name; quoting a name that could have been written bare is
SB5096, and a double-quoted head remains SB5011 because interpolation leaves
no single name to resolve. An *empty* name is SB4074, a syntax error rather
than a resolution one (DG, 2026-10-11): a zero-length name can denote neither
an intrinsic nor a program, and quoting says a name with no bare spelling, not
the absence of a name.

### Source emission

`--extract <binary> [source|ir|manifest]` prints one of the note sections
that `codegen/entry.rs::embed_artifacts` writes into every native binary, so
a compiled program always carries the source it was built from, its IR
capture, and its manifest (DD#45, DD#109). The section defaults to `source`,
so the original `--extract <binary>` spelling is unchanged.

Because a zero-length section does not survive into the linked ELF, a program
with no external dependencies has no `.sash_manifest` at all; that reads back
as an empty manifest rather than an error (scoped by `.sash_source`, so a
non-sash binary still fails).

Embedding the IR on every compile is what keeps the exhaustive emitter honest:
a variant that fails to render breaks every build instead of only the rare
`--emit-ir` invocation.

`--emit-source` and `--annotate` are **sash-convert** flags, not `sash`
flags (DD#57) — `sash` rejects them and says so. Source transformation is
the converter's job; the compiler only compiles.

---

## Tests

**The spec suite is the authoritative gate.** `cd spec && bash run.sh` runs
every `spec/*.sh` in both the native and C modes and accumulates results in
`spec.db` (SQLite, repo root, untracked — every run rewrites it wholesale,
and `runs` records a `commit_hash` per test so the database is its own
history). It is currently 4542 tests across 41 files.

`sash/tests/` **no longer exists.** All 546 Rust integration tests, and the
`sash-rt-tests` crate, were deleted in the 2026-09-14 redux (journal 304)
after each was classified with per-test evidence: 210 asserted surfaces we
have deliberately retired, 272 were already covered by concrete spec ids, 45
were bash-differentials with no independent assertion, and only 4 expressed
anything a spec could not — those became 20 new spec tests.

The reason for going that far, rather than repairing: **the harness was
fail-open.** `run_bash_native` called `sash::codegen::compile_to_object`
directly, so `validate` never ran. 49 of its 159 passing tests (31%)
compiled programs the real compiler rejects — `echo -n`, for instance, is
rejected by SB5059 but still honoured by codegen. A third of that suite's
green was measuring a pipeline with a pass removed.

> If you build a new harness, **drive the real `sash` binary**. A harness
> that calls into the middle of the pipeline tests a compiler we do not ship.

`mk test` (Plan 9 mk, not make) covers what remains: 152 C regex checks, 26
`sash-grep` checks, and 77 in-crate unit tests (`sash` 30, `sash-convert`
27, `sash-expand` 20). It is green.

---

## Key Design Decisions

Design decisions documented in `notes/decisions/`. The most
architecturally significant:

| # | Decision | Impact |
|---|----------|--------|
| 12 | Strings are (ptr, len) with null terminator | C interop without copying |
| 104 | Janet oracle decoupled via `--emit-ir` (supersedes DD#15) | Oracle is out-of-tree, post-beta |
| 24 | Taint quarantine | All external input tainted |
| 30 | Typed variables (int) | `declare int`, `fn -> int` |
| 33 | Unified hash table | SashTable for indexed + associative arrays |
| 34 | Minimize unsafe in sash-rt | Audit surface reduction |
| 39 | Star-only glob + regex mutation | `${var:s/re/str/flags}` |
| 43 | Attest semantics | Validation-gated declassification |
| 44 | Full taint quarantine | Read-blocked, not just command-blocked |
| 45 | Source emitter architecture | Four-axis declarations, const, export-as-error |

---

## Adversarial Review

One completed (audit/run-1/). 5 models, 3 passes, remediation done.
Validator bypass testing is the highest priority for future reviews —
finding Bash scripts that achieve dangerous effects while passing
the validator without diagnostics.

---

## For New Contributors

1. Read `notes/journal/001-prologue.md`. It sets the tone for everything
   else.
2. Read `notes/status.md` — the current priority and what was just
   finished. Read it first every session, not just the first one.
3. Read this guide.
4. Run the spec suite (`cd spec && bash run.sh`) to see where the tree
   stands. That is the authoritative gate; `mk test` currently does not
   pass at HEAD (see the Tests section).
5. Read `notes/decisions/` before changing any behaviour that might be
   deliberate. It is the authoritative record of what is intentional
   versus what is a gap — and the safe subset is defined by those
   decisions, not by any single prose document.
6. Pick a task from `notes/tasks/`. **Re-measure the filing against the
   binary before acting on it** — a filing is a snapshot of a moving
   tree, and a surprising number of them have aged out.
7. Follow the completion checklist in `AGENTS.md`.
