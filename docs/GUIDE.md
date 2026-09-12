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
│   │   └── known_externals.rs — Auto-attest patterns
│   └── tests/
│       ├── codegen_tests.rs  — codegen correctness
│       ├── e2e_tests.rs      — end-to-end compilation and execution
│       ├── janet_tests.rs    — Janet round-trip
│       └── oracle_tests.rs   — oracle validation
├── sash-convert/          — Bash → sash-native converter
│   └── src/
│       ├── main.rs        — CLI entry point
│       └── convert.rs     — CST-walking conversion engine
├── sash-rt/               — Runtime library (#![no_std], rustix, no libc)
│   └── src/lib.rs         — ~2300 lines
├── sash-expand/           — Expansion utilities
├── docs/                  — Design docs (GUIDE, TIMELINE)
├── notes/                 — iwe knowledge graph (journal, decisions, tasks)
├── audit/                 — Adversarial review results
│   └── run-1/             — 5 models, 3 passes, remediation done
├── spec/                  — Spec test suite (236 passing)
├── contrib/               — Vendored: plan9port-regexp, freertos-heap4
├── suites/                — brush/oils test runners
├── exp/                   — Git submodule (safebash-exp, optional)
└── mkfile                 — Plan 9 mk build system (build, test, clean)
```

The tree-sitter grammar lives externally at
`~/src/tree-sitter-bash-safebash` — a fork of tree-sitter-bash with
sash extensions (int, const, array, map, attest, fn,
`${var:s/re/str/}`).

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
`$#`, `$?`, `$$`, variables assigned from clean expressions.

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

- `int x=5` / `declare int x=5` / `declare -i x` — int variables
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
For, Case (the `[[ ]]` dispatch form, DD#66), Function, Redirected,
Subshell, Group, ArithmeticCommand.

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
compile error (SB4033). The Janet backend shells to the `sash-regex` CLI
(`captures` op) so the oracle uses the same engine.

### I/O

`sash_echo`, `sash_write`, `sash_read_line`,
`sash_heredoc_redirect`.

### Process management

`sash_exec` — execve with absolute path (no PATH walk; DD#23). `sash_capture_start`/
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

Test files in `sash/tests/`:

| File | What it tests |
|------|---------------|
| `codegen_tests.rs` | Compile bash snippets to native, run, check output |
| `e2e_tests.rs` | Positive (compile+run), negative (expect errors), coercion, annotation, extract, auto-attest |
| `janet_tests.rs` | Compile to Janet, run both bash and Janet, compare output |
| `oracle_tests.rs` | Additional validation tests |
| `taint_spec_tests.rs` | Taint model: rejection of tainted reads, attest runtime enforcement, DD#46 positionals |
| `brush_compat_tests.rs` | Brush shell compatibility |

Build and run: `mk test` (Plan 9 mk, not make).

---

## Key Design Decisions

Design decisions documented in `notes/decisions/`. The most
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

1. Read `JOURNAL.md` prologue (first 40 lines).
2. Read this guide.
3. Run `mk test` to verify everything passes.
4. Pick a task from `TODO.md`.
5. Follow the completion checklist in `AGENTS.md`.
