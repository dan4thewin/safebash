# safebash — A Guide to the Architecture

*For anyone continuing or contributing to this project.*

This document explains how safebash works and why it is designed the
way it is. It is written for someone with a working knowledge of
compilers and Rust, but not necessarily deep expertise in either.

**A note on authorship:** This guide was written by an AI (Claude,
Anthropic) working under the direction of DG, the human creator of
safebash. The AI wrote the prose and the code; DG directed the
effort and made design decisions. See the Prologue of `JOURNAL.md`
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
   at all until validated via `bless VAR EXPR`. Environment variables
   are accessed via `$(getenv NAME)`, positional params via `$(argv N)`.
   No "variables just appearing" — every value has a visible origin.

3. **Supply-chain safety.** Compiled artifacts are signed. The system
   refuses to run binaries whose signature or integrity check fails.

Safety does NOT mean sandboxing. External commands (`rm`, `mount`,
etc.) are legitimate operations. Safety means the script's intent is
visible, its data boundaries are explicit, and its binary is
tamper-evident.

---

## Repository Structure

```
safebash/
├── sash/                  — The compiler (Rust crate)
│   ├── src/
│   │   ├── main.rs        — Entry point, linking
│   │   ├── lib.rs         — Library entry
│   │   ├── lower.rs       — CST → IR lowering with coercions
│   │   ├── ir.rs          — IR definitions (Stmt, Expr)
│   │   ├── type_check.rs  — Type checking (int vs string)
│   │   ├── transform.rs   — IR → SSA transformation
│   │   ├── ssa.rs         — SSA representation
│   │   ├── ssa_printer.rs — SSA pretty-printing
│   │   ├── codegen.rs     — Cranelift code generation (~4200 lines)
│   │   ├── taint.rs       — Flow-sensitive taint analysis
│   │   ├── validator.rs   — Validation orchestrator
│   │   ├── rule.rs        — Rule trait
│   │   ├── rules/         — Validation rules (SB4001–SB5002)
│   │   ├── diagnostic.rs  — Error reporting
│   │   ├── emit_janet.rs  — Janet backend
│   │   ├── emit_source.rs — Source emission modes
│   │   ├── printer.rs     — Source printer
│   │   └── known_externals.rs — Auto-bless patterns
│   └── tests/
│       ├── codegen_tests.rs  — 247 codegen correctness tests
│       ├── e2e_tests.rs      — 116 end-to-end tests
│       ├── janet_tests.rs    — 37 Janet round-trip tests
│       └── oracle_tests.rs   — 59 oracle validation tests
├── sash-rt/               — Runtime library (#![no_std], rustix, no libc)
│   └── src/lib.rs         — ~2500 lines
├── docs/                   — Design docs (GUIDE, JOURNAL, SUBSET, TODO, TIMELINE)
├── audit/                 — Adversarial review results
│   └── run-1/             — 5 models, 3 passes, remediation done
├── contrib/               — Vendored: plan9port-regexp, freertos-heap4
├── suites/                — brush/oils test runners
├── exp/                   — Git submodule (safebash-exp, optional)
└── mkfile                 — Plan 9 mk build system (build, test, clean)
```

The tree-sitter grammar lives externally at
`~/src/tree-sitter-bash-safebash` — a fork of tree-sitter-bash with
sash extensions (int, const, array, map, bless, fn,
`${var:s/re/str/}`).

---

## Compilation Pipeline

```
 Bash source (.sh)
       │
  [1]  │  Parse
       ▼
 tree-sitter-bash-safebash → CST
       │
  [2]  │  Validate
       ▼
 rule.rs + rules/*.rs → diagnostics (SB4001–SB5002)
       │
  [3]  │  Taint
       ▼
 taint.rs → flow-sensitive taint tracking (SB4020)
       │
  [4]  │  Lower
       ▼
 lower.rs → IR (ir.rs) with coercions
       │
  [5]  │  Type check
       ▼
 type_check.rs → int vs string inference, declare -i stickiness
       │
  [6]  │  Transform
       ▼
 transform.rs → SSA (ssa.rs): versioning, phi nodes, basic blocks, CFG
       │
  [7]  │  Fold
       ▼
 Constant folding in compile_to_object
       │
  [8]  │  Codegen
       ▼
 codegen.rs → Cranelift IR → object code (.o)
       │     Declares extern functions from sash-rt
       │     Embeds .sash_source ELF section
       │
  [9]  │  Link
       ▼
 main.rs invokes ld → object + libsash_rt.a → static ELF binary
```

### Phase 1: Parse

tree-sitter-bash-safebash produces a concrete syntax tree (CST) —
every token, every piece of punctuation preserved losslessly.

### Phase 2: Validate

Walks the CST once, dispatching each node to registered rules.
Rules implement the `Rule` trait. Only subset gate rules remain —
coercion rules were removed because sash handles those constructs
silently (DD#40).

| Rule file | Codes | What it catches |
|-----------|-------|-----------------|
| `dynamic_execution.rs` | SB4001 | `eval` (reject) |
| `dynamic_execution.rs` | SB4002 | `source`/`.` (marker) |
| `export_rejected.rs` | SB5001 | `export` (reject, DD#45) |
| `heredoc_cmdsub.rs` | SB5002 | Command substitution in heredocs |

Wrapper unwrapping: `builtin`, `command`, and `env` prefixes are
stripped before checking the effective command. ANSI-C quoted
strings (`$'\x65\x76\x61\x6c'`) are decoded before comparison.

### Phase 3: Taint

Flow-sensitive taint analysis (DD#24, DD#42–44). Walks the program
sequentially, tracking each variable as `Tainted`, `Blessed`, or
`Clean`.

**Taint sources:** positional params (`$1`–`$N`, `$@`), `read`
targets, `$(external cmd)`, environment variables via `$(getenv NAME)`,
`$(argv N)`.

**Clean sources:** literals, arithmetic from clean operands, `$0`,
`$#`, `$?`, `$$`, variables assigned from clean expressions.

**Auto-bless:** `known_externals.rs` provides auto-bless patterns
for 26 environment variables and 25 commands.

**`bless VAR EXPR`** — evaluates EXPR (tainted VAR readable inside
the expression). If truthy, marks VAR clean. If falsy:
- Bare `bless` → fatal exit
- `if bless` → false branch taken

```bash
port=$(argv 1) || { echo "usage: $0 port"; exit 1; }
bless port [[ $port =~ ^[0-9]+$ ]]
echo $port   # clean — blessed above
```

**The rule:** you can write tainted data into a variable but cannot
read it except inside `bless`. Diagnostic: SB4020.

### Phase 4: Lower

`lower.rs` converts CST → IR. This is where coercions happen:

- `declare` → typed declarations (int, array, map)
- `let` → `(( ))`
- Glob patterns → regex
- Backticks → `$()`
- `[ ]` → `[[ ]]`

Also drives `emit_source.rs` for `--emit-source` output.

### Phase 5: Type Check

`type_check.rs` infers int vs string for each variable. Tracks
`declare -i` stickiness. Cross-type assignment without conversion
is a compile-time error.

- `declare int x=5` / `local int x` / `declare -i x` — int variables
- `fn name() -> int { }` — int-returning functions
- `$#`, `$?`, `$((...))` — intrinsically int

### Phase 6: Transform

`transform.rs` converts IR → SSA. Variable versioning, phi nodes at
merge points, basic blocks with explicit terminators, control flow
graph construction. Function inlining. Arithmetic evaluation.

### Phases 7–9: Fold, Codegen, Link

Constant folding, then `codegen.rs` (~4200 lines) lowers SSA to
Cranelift IR. Declares extern functions from `sash-rt`. Embeds the
original source in a `.sash_source` ELF section. Finally, `main.rs`
invokes `ld` to link the object with `libsash_rt.a` into a fully
static ELF binary.

---

## IR (Intermediate Representation)

Defined in `ir.rs`.

### Statement types (StmtKind)

Assignment, Declaration, Command, Pipeline, List (&&/||), If, While,
For, Case, Function, Redirected, Subshell, Group, ArithmeticCommand.

Subshell exists in the IR for bash-mode input but is coerced during
lowering into sash primitives (DD#38).

### Expression types (ExprKind)

Literal, SimpleExpansion, DoubleQuoted, SingleQuoted,
CommandSubstitution, ArithmeticExpansion, Concat,
ParameterExpansion, Array, AssocArray, Subscript, Test.

---

## SSA (Static Single Assignment)

Defined in `ssa.rs`.

- **SsaProgram** — functions, each containing `Vec<BasicBlock>`
- **SsaInst** — Assign, Command, Pipeline, Test, ReadLine,
  ReadLineMulti, CaptureStart/End, Export, Bless, Trap, etc.
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

`sash-rt/src/lib.rs` (~2500 lines) — a `#![no_std]` Rust crate.
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
`sash_regex_replace`, `sash_fnmatch`.

All strings are `(ptr, len)` pairs with a null terminator at
`ptr[len]`. This gives length-bounded operations for safety and
null-terminated C interop for `execve` without copying (DD#12).

### Regex

Plan 9 regex engine, vendored from `contrib/plan9port-regexp`.
Supports `${var:s/re/str/flags}` syntax (DD#39).

### I/O

`sash_echo`, `sash_write`, `sash_read_line`,
`sash_heredoc_redirect`.

### Process management

`sash_exec` — PATH lookup, fork/exec. `sash_capture_start`/
`sash_capture_end` — command substitution via pipe + fork.

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

### Janet

`emit_janet.rs` — transpile IR to Janet for oracle testing. 37
round-trip tests compile to both Janet and native, run both, and
compare output.

The Janet oracle is the normative semantics of safebash (DD#15).
When the oracle and bash disagree, the question is not "which is
right?" but "which behavior does safebash want, and why?"

### Source emission

`emit_source.rs` — emit coerced sash-native source. Three modes:
`--emit-source`, `--annotate`, `--extract`.

---

## Tests

457 tests total across four test files in `sash/tests/`:

| File | Count | What it tests |
|------|-------|---------------|
| `codegen_tests.rs` | 247 | Compile bash snippets to native, run, check output |
| `e2e_tests.rs` | 116 | Positive (compile+run), negative (expect errors), coercion, annotation, extract, auto-bless |
| `janet_tests.rs` | 37 | Compile to Janet, run both bash and Janet, compare output |
| `oracle_tests.rs` | 59 | Additional validation tests |
| `taint_spec_tests.rs` | 43 | Taint model: rejection of tainted reads, bless runtime enforcement, DD#46 positionals |

Build and run: `mk test` (Plan 9 mk, not make).

---

## Key Design Decisions

45 design decisions documented in `docs/JOURNAL.md`. The most
architecturally significant:

| # | Decision | Impact |
|---|----------|--------|
| 12 | Strings are (ptr, len) with null terminator | C interop without copying |
| 15 | Janet oracle defines safebash semantics | Model is the specification |
| 24 | Taint quarantine | All external input tainted |
| 30 | Typed variables (int) | `declare int`, `fn -> int` |
| 33 | Unified hash table | SashTable for indexed + associative arrays |
| 34 | Minimize unsafe in sash-rt | Audit surface reduction |
| 39 | Star-only glob + regex mutation | `${var:s/re/str/flags}` |
| 43 | Bless semantics | Validation-gated declassification |
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

1. Read `JOURNAL.md` prologue (first 40 lines).
2. Read this guide.
3. Run `mk test` to verify everything passes.
4. Pick a task from `TODO.md`.
5. Follow the completion checklist in `AGENTS.md`.
