# safebash — Timeline

A chronological record of the safebash compiler, built experiment by
experiment from a blank Rust project to a signing-capable compiler.
Each entry marks what became possible that wasn't before.

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

## Phase 1: Toolchain Bootstrap (Experiments 001–005)

*2026-03-26. Five experiments in one session.*

*Proving that Rust + tree-sitter-bash can parse real deployment scripts.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 001 | cargo init | Rust toolchain proven — binary named `sash` |
| 002 | tree-sitter dep | Bash grammar loads via FFI — 25 crates, C compilation invisible |
| 003 | Parse hello | First successful parse — `program` > `command` structure |
| 004 | Walk CST | Full tree structure visible — `simple_expansion`, `test_command`, punctuation nodes |
| 005 | Parse script | Real-world coverage — functions, for loops, pipelines, traps, command substitution |

---

## Phase 2: Safe Subset Validation (Experiments 006–011)

*2026-04-11. Six experiments establishing the validator.*

*From ShellCheck survey to unified rule framework — defining what bash
constructs the safe subset permits and rejecting the rest.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 006 | ShellCheck survey → SUBSET.md | ~60 SC codes classified, 9 categories |
| 007 | Reject eval (first rule) | SB4001–SB4004, Diagnostic struct established |
| 008 | Unquoted expansion (SC2086/SC2046) | Ancestor-chain safety checks |
| 009 | Backtick substitution (SC2006) | Simplest rule — first-child check |
| 010 | Single bracket test (SC2292) | test_command + test builtin detection |
| 011 | Validator framework | Unified Rule trait, single CST walk, 5 rules |

---

## Phase 3: IR Construction (Experiments 012–015)

*2026-04-11. Four experiments building the intermediate representation.*

*Lowering tree-sitter's CST into a typed IR suitable for analysis,
transformation, and code generation.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 012 | IR types | 14 stmt types, 10 expr types, RedirectOp, TestExpr |
| 013 | CST→IR lowering | Assignments, declarations, commands, expressions |
| 014 | Complete CST→IR | Pipelines, if/while/for/case, functions, redirects, subshells |
| 015 | IR printer | Pretty-print all IR constructs, 41 tests |

---

## Phase 4: SSA Transformation (Experiment 016)

*2026-04-12. Single experiment.*

*Converting the IR to Static Single Assignment form — versioned
variables, phi nodes, basic blocks, and terminators.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 016 | SSA transform | Versioned vars, phi nodes, basic blocks, terminators, version counter fix |

---

## Phase 5: Janet Oracle (Experiment 017)

*2026-04-12. Single experiment.*

*External correctness verification: compile Bash→IR→Janet, execute
in Janet, compare output with Bash.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 017 | Janet oracle | Bash→IR→Janet→verify, 28 oracle tests, 4 bugs fixed |

---

## Phase 6: Subshell Elision (Experiment 018)

*2026-04-12. Single experiment.*

*Analyzing command substitutions to classify them as elidable (pure
function calls, constant) vs required (external commands, side effects).*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 018 | Elision analysis | Classify $(cmd) as elidable vs required, function analysis |

---

## Phase 7: Adversarial Audit (Experiments 019–020, audit/run-1)

*2026-04-12. Multi-model audit plus two remediation experiments.*

*Five models × three passes identified bypass vectors. Critical and
HIGH findings triaged and fixed.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| audit | Multi-model audit | 5 models × 3 passes, triage, 2 remediation plans, reconciliation |
| 019 | Critical bypass fixes | B1–B4: builtin/command unwrap, bash -c, trap, ANSI-C decode |
| 020 | HIGH gap fixes | Alias restriction, coproc/select rejection |

---

## Phase 8: Cranelift AOT (Experiments 021–026)

*2026-04-12–13. Six experiments producing native ELF binaries.*

*From first Cranelift hello-world to a complete ahead-of-time compiler
pipeline. sash-rt/ crate created as #![no_std] staticlib via rustix.
13 design decisions documented in JOURNAL.md.*

| Exp | Milestone | What it unlocked |
|-----|-----------|-----------------|
| 021 | Cranelift hello world | First native binary from Cranelift |
| 022/022a | String variables | (ptr,len) pairs, null-terminated (Decision #12) |
| 023/023a/023b | External commands | C runtime → raw syscalls → pure Rust rustix |
| 024 | Control flow | if/else→blocks/branches, for→unrolling, inline string comparison |
| 025 | Elidable cmdsub | Constant folding: $(func) → compile-time literal, SsaValue::Concat |
| 026 | End-to-end AOT | Full pipeline, demo.sh, 16KB static ELF, bash-identical output |
