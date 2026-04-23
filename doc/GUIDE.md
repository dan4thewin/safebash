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

2. **Data-level safety.** Taint quarantine (DD#24, DD#42-44). All
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

## Compilation Pipeline

The `sash` compiler implements six phases:

```
 Bash source (.sh)
       │
       ▼
 [1] tree-sitter-bash parser → CST
       │
       ▼
 [2] Validator (7 rules) → diagnostics
       │  Rejects eval, backticks, ${!var}, single bracket, etc.
       │  AOT: strict by default. JIT: coerce by default.
       │
       ▼
 [3] CST → IR lowering
       │  14 statement types, 10 expression types
       │  Handles all Bash constructs: assignments, commands,
       │  pipelines, if/while/for/case, functions, redirects
       │
       ▼
 [4] IR → SSA transformation
       │  Versioned variables (x_1, x_2, ...)
       │  Phi nodes at merge points (if/else, loop headers)
       │  Basic blocks with explicit terminators
       │
       ├──→ [Janet oracle] SSA → Janet → run → compare to Bash
       │    (correctness verification, not in compilation path)
       │
       ▼
 [5] SSA → Cranelift IR → object file (.o)
       │  String variables as (ptr, len) with null terminator
       │  echo → write syscall, external cmd → fork/execve
       │  Elidable $(func) → constant-folded at compile time
       │
       ▼
 [6] ld + libsash_rt.a → static ELF binary
       No libc, no dynamic linker
       Result: ~16KB statically linked executable
```

## Key Design Decisions

Thirteen design decisions are documented in `JOURNAL.md`. The most
architecturally significant:

| # | Decision | Impact |
|---|----------|--------|
| 1 | `source`/`.` compiled to safe KVP loader | No runtime code loading |
| 3 | `local -n` allowed, `${!var}` rejected | Analyzable vs opaque indirection |
| 4 | `$cmd` two-pass: CST literal check → SSA resolution | Three-state diagnostics |
| 8 | AOT strict, JIT coerce by default | Different defaults for different contexts |
| 10 | `bash -c 'literal'` compiles inline; variable arg rejected | "If I can see the code, I'll compile it" |
| 11 | Aliases rejected in AOT, converted to functions in JIT | Macros → analyzable functions |
| 12 | Strings are (ptr, len) with null terminator | C interop without copying |

---

## Parsing

safebash uses **tree-sitter-bash** (v0.23) for parsing. tree-sitter
is a C library with Rust bindings via the `tree-sitter` crate (v0.25).

### What tree-sitter gives us

A **concrete syntax tree** (CST) — every token, every piece of
punctuation, every whitespace position is preserved. This is not an
abstract syntax tree; it's a lossless representation of the source.

### Key node types

| Bash construct | CST node type | Notes |
|---|---|---|
| `echo "hello"` | `command` | `command_name` + argument siblings |
| `x="val"` | `variable_assignment` | `variable_name` + `=` + value |
| `"$x"` | `string` > `simple_expansion` | Expansion inside quotes |
| `$x` (bare) | `simple_expansion` | Expansion without quotes — SC2086 |
| `if ...; then ...; fi` | `if_statement` | `test_command` or `command` as condition |
| `[ -f "$x" ]` | `test_command` > `unary_expression` | Operator + argument |
| `[[ ... ]]` | `test_command` (with `[[`) | Preferred form |
| `for x in ...; do ...; done` | `for_statement` | Variable + items + body |
| `while ...; do ...; done` | `while_statement` | Condition + body |
| `case $x in ...) ;; esac` | `case_statement` | Value + case_items |
| `cmd1 \| cmd2` | `pipeline` | Sequence of `command` nodes |
| `f() { ...; }` | `function_definition` | Name + `compound_statement` body |
| `$(cmd)` | `command_substitution` | Command inside expansion |
| `` `cmd` `` | `command_substitution` | First child is backtick (rejected) |
| `#!/bin/bash` | `comment` | First line if shebang |

### Known quirk

In double-quoted strings like `"$x $y"`, tree-sitter includes the
space before `$y` inside the second `simple_expansion` node's `$`
token. The `lower_string` function handles this by detecting
whitespace prefixes and emitting them as separate literal parts.

---

## Validator

The validator has two phases:

### Phase 1: Subset gate rules

Walks the CST once, dispatching each node to registered rules.
Rules implement the `Rule` trait. Per DD#40, only **subset gate**
rules remain — coercion rules (backticks, `[ ]`, unquoted expansion)
were removed because sash handles those constructs silently.

| Rule file | Codes | What it catches |
|-----------|-------|-----------------|
| `dynamic_execution.rs` | SB4001, SB4002, SB4004 | eval (reject), source (marker), $cmd (deferred) |
| `indirect_expansion.rs` | SB4003 | `${!var}` (reject) |

Diagnostic severity:

```rust
pub enum Severity {
    Error,     // must fix (eval, ${!var})
    Deferred,  // needs SSA to resolve ($cmd with readonly target)
    Marker,    // informational (source → kvp transform)
}
```

### Phase 2: Taint tracking (DD#24, DD#42-44)

Flow-sensitive taint analysis walks the program sequentially, tracking
each variable as `Tainted`, `Blessed`, or `Clean`.

**Taint sources:** positional params (`$1`–`$N`, `$@`), `read` targets,
`$(external cmd)`, undeclared variables (env vars via `$(getenv NAME)`),
`$(argv N)`.

**Clean sources:** literals, arithmetic from clean operands, `$0`, `$#`,
`$?`, `$$`, variables assigned from clean expressions, clean builtins
(`len`, `keys`, `join`, `epochseconds`).

**The rule:** you can **write** tainted data into a variable (assignment
is a write context). You cannot **read** it except inside `bless`.

```bash
port=$(argv 1) || { echo "usage: $0 port"; exit 1; }
bless port [[ $port =~ ^[0-9]+$ ]]
echo $port   # clean — blessed above
```

**`bless VAR EXPR`** — evaluates EXPR (tainted VAR readable inside
the expression). If truthy, marks VAR clean. If falsy:
- Bare `bless` → fatal exit
- `if bless` → false branch taken

Diagnostic codes:
- SB4010: `bless` missing variable name or expression
- SB4011: blessing an already-clean variable (unnecessary)
- SB4020: use of tainted variable outside bless context

### Wrapper unwrapping

`dynamic_execution.rs` unwraps `builtin`, `command`, and `env`
prefixes before checking the effective command. `builtin eval`,
`command eval`, and `env eval` all trigger SB4001. ANSI-C quoted
strings (`$'\x65\x76\x61\x6c'`) are decoded before comparison.

---

## IR (Intermediate Representation)

The IR is a structured Rust representation of the Bash program.

### Statement types (14)

Assignment, Declaration, Command, Pipeline, List (&&/||), If, While,
For, Case, Function, Redirected, Subshell, Group, Comment.

### Expression types (10)

Literal, DoubleQuoted, SingleQuoted, SimpleExpansion, ParameterExpansion,
CommandSubstitution, ArithmeticExpansion, Concat, Array, Test.

### Lowering

`lower.rs` converts tree-sitter CST nodes to IR. Notable complexity:

- **Double-quoted strings** with mixed literal/expansion parts need
  gap detection (byte-range analysis) to capture spaces between
  `$x` and `$y`.
- **Declaration commands** (`declare`, `local`, `export`, `readonly`)
  parsed as separate statement type with flags and assignments.
- **Test expressions** (`[[ ]]`) preserved as structured `TestExpr`
  nodes (Unary, Binary, Not, And, Or, Paren).

---

## SSA (Static Single Assignment)

### Types

- **VarRef** — `{ name: String, version: u32 }`
- **PhiNode** — `{ target: VarRef, sources: Vec<(BlockId, VarRef)> }`
- **SsaInst** — Assign, Command, Phi, Test
- **SsaValue** — Literal, Var, Expr, FuncCall, Concat
- **BasicBlock** — label, phis, instructions, terminator
- **Terminator** — Goto, Branch, Return, Loop

### Version state

`VersionState` tracks variable versions. The `counter` field is a
high-water mark that never rolls back — even when restoring snapshots
from branches. This prevents version collisions across if/else arms.

### Elidable command substitution

The SSA transform detects when a `$(func)` calls a known function
and emits `SsaValue::FuncCall(name)`. The codegen constant-folds
function blocks: if every instruction is `echo` with literal args,
the output becomes a compile-time constant. The function body is
never emitted to the native binary.

---

## Cranelift Codegen

`codegen.rs` lowers SSA to native machine code via Cranelift.

### String representation

All strings are `(ptr, len)` pairs where the buffer has `len+1`
bytes — the extra byte is always `\0`. This gives length-bounded
operations for safety and null-terminated C interop for `execve`
without copying. (Decision #12.)

### Runtime (sash-rt)

`sash-rt/` is a standalone `#![no_std]` Rust crate compiled to a
static library. It uses `rustix` for raw Linux syscalls — no libc.

Exports (composable primitives — Design Decision #14):
- `sash_write(fd, buf, len)` — write syscall
- `sash_exit(code)` — exit_group syscall
- `sash_fork() → pid` — fork (0=child, >0=parent, -1=error)
- `sash_exec(argv, envp) → !` — execve with PATH lookup
- `sash_wait(pid) → exit_status` — waitpid
- `sash_pipe(out_ptr) → 0|-1` — create pipe [read_fd, write_fd]
- `sash_dup2(src_fd, dst_fd) → 0|-1` — duplicate fd
- `sash_redirect(fd, path, flags) → 0|-1` — open + dup2
- `sash_close(fd) → 0|-1` — close fd
- `sash_read(fd, buf, len) → bytes_read` — read from fd
- `sash_alloc(n) → ptr` — bump allocator (128KB heap)
- `sash_getenv(name, name_len, &out_len) → ptr` — environment variable lookup
- `sash_strip_suffix(s, s_len, pat, pat_len, longest) → new_len` — `%%`/`%`
- `sash_strip_prefix(s, s_len, pat, pat_len, longest) → offset` — `##`/`#`
- `sash_default_value(s_len) → 0|1` — `:-` operator (empty check)
- `strlen(ptr) → len` — null-terminated string length
- `_start` — minimal entry point (naked asm, x86_64 + aarch64)

Legacy wrappers (built on composable primitives):
- `sash_run(argv, envp) → status` — fork + exec + wait
- `sash_capture(argv, envp, &out_len) → ptr` — fork + pipe + exec + read + wait

Linked into the final binary via `ld`. The result is a fully static
ELF executable with zero external dependencies.

### What codegen handles today

- Variable assignment (string store)
- `echo` with literal, variable, and Concat arguments
- External commands via `sash_run` (fork/execve)
- `if`/`else` with `[[ ]]` string comparison (`=`, `!=`, `-n`, `-z`)
- `for` loops with literal word lists (compile-time unrolling)
- Elidable `$(func)` (constant folding)
- Positional parameters (`$1`, `$2`, ...) and env vars (`$HOME`)
- `exit N` builtin
- Parameter expansion: `${x%%p}`, `${x%p}`, `${x##p}`, `${x#p}`, `${x:-d}`
- Runtime command substitution: `x=$(cmd args...)` via fork/pipe/exec/read

### Type system (DD#16, DD#17)

safebash distinguishes two types: **string** (default) and **int**.

- `declare int x=5` / `local int x` / `declare -i x` — int variables
- `fn name() -> int { }` — int-returning functions (`return` carries full i64)
- `$#`, `$?`, `$((...))` — intrinsically int
- String→int: `$(($var))` (arithmetic context)
- Int→string: `"$var"` (interpolation context)
- Cross-type assignment without conversion is a compile-time error
- `fn` keyword alias for `function`
- Grammar: forked tree-sitter-bash with `type_annotation` node and `-> int` syntax
- Type checker: `type_check.rs` — validates assignments, exit args, boolean context

---

## Janet Oracle

The Janet oracle (`exp/017-janet-oracle/`, updated in `030-*`) emits
Janet code from the IR.  Its role has evolved:

**The oracle is the normative semantics of safebash.**

Safebash is not a bug-for-bug reimplementation of bash.  It compiles
a safe subset — and may refuse functionality or replace it with a safe
alternative, so long as the alternative does what the programmer
*intended*, safely.  That means bash itself is not the specification.
The *model* is the specification.  Bash is one implementation of that
model, carrying 30 years of accumulated warts that safebash
deliberately rejects.

The Janet emission makes the model visible and inspectable:

```
Bash source → IR → Janet code    ← "what does this program mean?"
                  → Cranelift    ← "what does the native binary do?"
```

Testing against bash checks behavior for a given set of inputs.
The Janet oracle externalizes the *reasoning* — you can read the
emitted Janet and see whether it correctly captures the intent of the
bash construct.  `(string/slice x 0 (string/find "/" x))` is a
readable claim about what `${x%%/*}` means.  If that claim is wrong,
you can see it by reading, even without running.

When the oracle and bash disagree, the question is not "which is
right?" but "which behavior does safebash want, and why?"  Every
such disagreement is a design decision worth documenting.

Janet was chosen because: tiny binary (~300KB), S-expression syntax
makes IR structure visible, lexical scoping aligns with SSA, built-in
POSIX primitives (`os/execute`, `os/spawn`, `os/pipe`).

Design Decision #15: The Janet oracle defines safebash semantics.
Bash comparison tests verify compatibility; the oracle verifies
understanding.

---

## Subshell Elision

`exp/018-subshell-elision/` implements static analysis that classifies
`$(...)` and `(...)` constructs as:

- **Elidable** — body contains only builtins and known functions.
  Can be compiled as a function call returning a string, no fork needed.
- **Required** — body contains external commands, pipelines, or
  background jobs. Must spawn a real subprocess.

This analysis feeds the Cranelift codegen: elidable command
substitutions compile to constant-folded string literals (experiment
025), while required ones compile to fork/exec.

---

## Directory Structure

```
safebash/
├── AGENTS.md              — AI contributor field notes
├── README.md              — Project overview
├── sash-rt/               — Runtime library crate
│   ├── Cargo.toml
│   └── src/lib.rs         — #![no_std], rustix syscalls, _start
├── audit/
│   └── run-1/             — Adversarial audit artifacts
│       ├── PROMPT_*.txt   — Audit prompts (bypass, IR, spec)
│       ├── bypass_*.txt   — Model responses (5 models)
│       ├── ir_*.txt
│       ├── spec_*.txt
│       ├── AUDIT_TRIAGE.md
│       ├── remediation_plan_*.txt
│       └── reconciliation_*.txt
└── exp/
    ├── GUIDE.md           — This file
    ├── JOURNAL.md         — Detailed experiment journal
    ├── TIMELINE.md        — Chronological milestone record
    ├── TODO.md            — Backlog
    ├── SUBSET.md          — Safe subset specification
    ├── mkfile             — mk testall runner
    └── NNN-name/          — Individual experiments (001–031)
        ├── Cargo.toml
        ├── mkfile
        ├── src/
        └── tests/
```

---

## For New Contributors

1. Read `JOURNAL.md` prologue (first 40 lines).
2. Read this guide.
3. Run `cd exp && mk testall` to verify everything passes.
4. Pick a task from `TODO.md`.
5. Create a new experiment directory: `exp/NNN-short-name/`.
6. Follow the completion checklist in `AGENTS.md`.

The experiment model: each experiment lives in its own directory,
has an `mkfile` with a `test` target, and is documented in the
journal. Don't skip ahead. Don't combine experiments unless the
scope is clearly related. Each one should pass before the next begins.
