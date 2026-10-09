# safebash — Timeline

A chronological record of the safebash compiler, built experiment by
experiment from a blank Rust project to a fully consolidated compiler
producing static native binaries from a safe subset of bash.

The project began on 2026-03-26 with a design document.

---

## Phase 0: Design (2026-03-26)

| Event | What it established |
|-------|-------------------|
| Design document | Safety model, chain of trust, architecture |
| Janet chosen | External correctness oracle (replacing newLISP) |
| Naming | Project: safebash, CLI binary: sash |
| Repository | github.com/dan4thewin/safebash (private) |

---

## Phase 1: Experiments (exp/001–exp/045)

### Toolchain Bootstrap (001–005)

*Proving that Rust + tree-sitter-bash can parse real deployment scripts.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 001 | cargo init | Rust toolchain proven — binary named `sash` |
| 002 | tree-sitter dep | Bash grammar loads via FFI |
| 003 | Parse hello | First successful parse — `program` > `command` structure |
| 004 | Walk CST | Full tree structure visible — expansions, test commands, punctuation nodes |
| 005 | Parse script | Real-world coverage — functions, for loops, pipelines, traps, command substitution |

### Safe Subset Validation (006–011)

*From ShellCheck survey to unified rule framework.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 006 | ShellCheck survey | ~60 SC codes classified, 9 categories → SUBSET.md |
| 007 | Reject eval | SB4001–SB4004, Diagnostic struct established |
| 008 | Unquoted expansion | SC2086/SC2046 — ancestor-chain safety checks |
| 009 | Backtick substitution | SC2006 — first-child check |
| 010 | Single bracket test | SC2292 — test_command + test builtin detection |
| 011 | Validator framework | Unified Rule trait, single CST walk, 5 rules |

### IR Construction (012–015)

*Lowering tree-sitter's CST into a typed IR.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 012 | IR types | 14 stmt types, 10 expr types, RedirectOp, TestExpr |
| 013 | CST→IR lowering | Assignments, declarations, commands, expressions |
| 014 | Complete CST→IR | Pipelines, if/while/for/case, functions, redirects, subshells |
| 015 | IR printer | Pretty-print all IR constructs, 41 tests |

### SSA and Oracle (016–017)

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 016 | SSA construction | Versioned vars, phi nodes, basic blocks, terminators |
| 017 | Janet oracle testing | Bash→IR→Janet→verify, 28 oracle tests, 4 bugs fixed |

### Analysis and Audit (018–020)

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 018 | Subshell elision | Classify $(cmd) as elidable vs required, function analysis |
| 019 | Critical bypass fixes | B1–B4: builtin/command unwrap, bash -c, trap, ANSI-C decode |
| 020 | HIGH gap fixes | Alias restriction, coproc/select rejection |

### Cranelift AOT (021–026)

*From first Cranelift hello-world to native ELF binaries. sash-rt/
created as `#![no_std]` staticlib via rustix (no libc).*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 021 | Cranelift hello world | First native binary from Cranelift |
| 022 | Strings (+ v2) | (ptr,len) pairs, null-terminated output |
| 023 | External commands (+ v2, rustix) | C runtime → raw syscalls → pure Rust rustix |
| 024 | Control flow codegen | if/else→blocks/branches, for→unrolling, inline string comparison |
| 025 | Elidable command substitution | Constant folding: $(func) → compile-time literal, SsaValue::Concat |
| 026 | End-to-end demo | Full pipeline, demo.sh, 16KB static ELF, bash-identical output |

### Feature Build-out (027–045)

*Systematic coverage of Bash semantics — each experiment adds a
capability, verified against Bash oracle output.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 027 | Positional parameters | $1, $2, …, $#, $0 |
| 028 | Parameter expansion | ${var:-default}, ${var:+alt}, ${#var}, ${var%pat}, etc. |
| 029 | Runtime command substitution | $(external_cmd) at runtime via fork/exec/pipe |
| 030 | Janet parity | Janet backend matches Bash output for full test suite |
| 031 | Type system | Static types, inference, coercion rules |
| 032 | Concat/while/numeric | String concatenation, while loops, numeric operations |
| 033 | Shift/export/$@ | shift builtin, export semantics, $@ expansion |
| 034 | Redirections | >, >>, <, 2>&1, file descriptor management |
| 035 | Command substitution | Nested and complex command substitution |
| 036 | Associative arrays | declare -A, key lookup, iteration |
| 037 | For/case/functions | for-in, case patterns, function definitions + calls |
| 038 | Dynamic for | for over variable-length lists |
| 039 | Until/while read | until loops, while read line idiom |
| 040 | Arithmetic statements | (( )), let, arithmetic evaluation |
| 041 | Int vars | declare -i, integer variable semantics |
| 042 | Exec lists/source | &&/\|\| lists, source/. builtin |
| 043 | Printf/arrays | printf builtin, indexed arrays |
| 044 | Pipe while read | cmd \| while read pattern |
| 045 | Validator taint bless | Taint tracking, bless semantics for safe data flow |

---

## Phase 2: Consolidated Compiler (sash/ crate)

After 45 experiments proved the architecture, the codebase was
consolidated into the `sash/` crate — a single compiler with
Cranelift and Janet backends, a pure-Rust `#![no_std]` runtime
(`sash-rt/`), and a forked tree-sitter grammar.

### Taint and Safety

- Taint tracking + bless semantics (DD#43–44)
- known_externals auto-bless for allowlisted commands
- Export rejection rule (SB5001)
- Heredoc command substitution rejection (SB5002)

### Language Extensions

- Declaration coercions and sash-native keywords: `int`, `const`, `array`, `map`
- `${var:s/re/str/flags}` — regex substitution grammar + compilation
- Tree-sitter grammar fork (`tree-sitter-bash-safebash`) with sash extensions

### Compiler Features

- Source emitter: `--emit-source`, `--annotate`, `--extract`
- `.sash_source` ELF section for embedded source recovery
- Function argument passing fix
- Constant folding pass

### Janet Backend

- Brought to full parity with Cranelift backend (37 tests)

### Infrastructure

- Repo split: safebash (compiler) + safebash-exp (experiments, git submodule)
- Adversarial audit (run-1): 5 models × 3 passes, triage, remediation
- Brush test suite integration (173 PASS baseline)
- 457 tests total (247 codegen, 114 e2e, 37 janet, 59 oracle)

---

## Stats

| Metric | Value |
|--------|-------|
| Experiment tests | ~520 |
| Consolidated compiler tests | 457 |
| Design decisions | 45 (DD#1–DD#45) |
| Adversarial audits | 1 completed (5 models, 3 passes) |
| AI conversation turns | ~3400+ |
| sash-rt | ~2500 lines pure Rust, no libc |
| sash compiler | ~12000+ lines Rust |
| tree-sitter-bash-safebash | Forked grammar with sash extensions |
