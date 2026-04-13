# Safe Bash Subset Compiler (Rust + Cranelift + SSA + Janet)

Shell scripts run in critical paths — deployment, provisioning, system
updates — but they cannot participate in a chain of trust. They cannot
be signed, verified, or refused by the kernel when tampered with. This
project changes that.

This is an experimental compiler for a safe, statically analyzable
subset of Bash, implemented in Rust, that produces native artifacts
whose integrity can be enforced at execution time.

The chain of trust:

    Author writes safe-subset script
        → Compiler produces deterministic native binary
            → Binary is signed in the build pipeline
                → System policy (IPE / fs-verity) verifies before execution

Unsigned or tampered artifacts are refused. The script's behavior is
visible in the source. The binary's integrity is guaranteed at runtime.

---------------------------------------------------------------------

## Current Status

The end-to-end pipeline works. Bash scripts compile to ~16KB static
ELF binaries with no libc dependency:

```
bash source → tree-sitter parse → 7 validator rules
    → IR (14 stmt types, 10 expr types)
        → SSA (versioned vars, phi nodes)
            → Cranelift codegen → ld + libsash_rt.a
                → static native binary
```

A demo script (`exp/026-end-to-end/demo.sh`) exercises variables,
functions, command substitution, for-loops, conditionals, and external
commands. The compiled binary produces byte-identical output to `bash`:

```bash
#!/bin/bash
name="safebash"
version="0.1.0"

get_status() {
    echo "operational"
}
status=$(get_status)

echo "$name v$version"
echo "status: $status"

for phase in parse validate lower ssa codegen link; do
    echo "  phase: $phase"
done

if [[ "$name" = "safebash" ]]; then
    echo "chain of trust: verified"
fi

uname -s
date +%Y-%m-%d
```

~520 tests across 30 experiments. 13 design decisions documented in
`exp/JOURNAL.md`.

---------------------------------------------------------------------

## Quick Start

```bash
# Build the runtime
cd sash-rt && cargo build --release && cd ..

# Compile and run demo.sh
cd exp/026-end-to-end
cargo run -- ../../exp/026-end-to-end/demo.sh   # produces static ELF
./demo                                           # run it

# Run all tests
cd exp && mk testall
```

---------------------------------------------------------------------

## Safety Model

Safety in this project has two dimensions:

1. **Language-level safety.** The safe subset eliminates constructs that
   make Bash difficult to analyze:
   - no dynamic eval
   - no unquoted expansions
   - no ambiguous globbing
   - no implicit type coercions
   - no environment-dependent behavior

   A reader can look at a safe-subset script and know what it does.
   The compiler can statically verify its structure.

2. **Supply-chain safety.** Compiled artifacts are signed and verified.
   The system refuses to execute binaries whose signature or integrity
   check fails.

These two properties reinforce each other: language-level safety means
the signed artifact's behavior is predictable; supply-chain safety means
the artifact has not been altered since it was reviewed and built.

**What safety does not mean:** The safe subset does not restrict which
external commands a script may invoke. `rm`, `cp`, `mount`, and any
other command are legitimate operations. The safety model ensures that
the *script's intent is visible and its binary is tamper-evident* — it
does not sandbox the operations the script performs.

---------------------------------------------------------------------

## Project Goals

1. **Security and Signing** — *designed, not yet implemented*

   Compiled safe Bash artifacts:
   - are produced as AOT-compiled native binaries ✅
   - are signed as part of the build pipeline *(planned)*
   - are verified at load or execution time by system policy *(planned)*

   The intent is to integrate with mechanisms like:
   - SELinux IPE (Integrity Policy Enforcement)
   - fs-verity or similar file integrity schemes

2. **Safe Subset of Bash** — *implemented*

   14 diagnostic codes enforced across 7 validator rules in 7 rule files:
   SB4001–SB4006, SB4008–SB4009, SB4012–SB4013, SC2006, SC2046, SC2086,
   SC2292.

   Two modes (Decision #8):
   - **Strict mode (AOT default)** — reject scripts that use unsafe
     constructs. The compiler reports what needs to change and why.
   - **Coerce mode (JIT default)** — automatically transform unsafe
     constructs into their safe equivalents where possible (e.g.,
     unquoted `$var` → `"$var"`, backtick substitution → `$()`).
     Error only for constructs with no safe counterpart.

   Notable design decisions:
   - `source`/`.` compiled away to a safe KVP loader (Decision #1)
   - `${!var}` indirect expansion rejected; `declare -n`/`local -n`
     namerefs allowed (Decisions #2, #3)
   - `bash -c 'literal'` compiled as inline subshell;
     `bash -c "$var"` rejected (Decision #10)
   - Aliases rejected in AOT, converted to functions in JIT
     (Decision #11)

3. **Rust Front-End** — ✅ *done*

   The compiler front-end:
   - Parses Bash syntax using tree-sitter-bash
   - Validates against the safe subset
   - Produces a structured IR (14 stmt types, 10 expr types)
   - Converts IR into SSA form

4. **SSA (Static Single Assignment)** — ✅ *done*

   - Versioned variables (every assignment creates a new version)
   - Phi nodes at control flow merge points
   - Basic blocks with explicit terminators (4 terminator types)
   - All bindings explicit, all data flow traceable

5. **Janet Correctness Oracle** — ✅ *done*

   17 passing Bash→Janet→verify tests (experiment 017).

   Janet is used as an external reference interpreter — an independent
   implementation of the language semantics that exists outside the
   compiler. If the compiler has a bug in its reasoning, the oracle
   exposes it.

   The oracle must be external. If the compiler author also writes the
   reference interpreter in the same language, the same assumptions and
   blind spots get baked into both sides. An independent language with
   its own runtime provides genuine verification.

   Reasons for choosing Janet:
   - tiny standalone binary (~300KB), fast startup
   - S-expression syntax makes IR structure visible and auditable
   - lexical scoping aligns naturally with SSA's explicit binding model
   - built-in POSIX primitives (os/execute, os/spawn, os/pipe)
   - actively maintained with a growing community
   - clean, predictable evaluation semantics

   The compiler emits a restricted subset of Janet:
   - no nested functions or closures
   - no macros
   - flat, imperative structure only

6. **Cranelift Backend** — AOT ✅ *done*, JIT *not started*

   AOT compilation produces ~16KB static ELF binaries linked against
   `sash-rt`, a pure Rust `#![no_std]` runtime using rustix (no libc,
   no C, no inline asm in safebash). The linker is the system `ld`
   (Decision #13).

   JIT for the REPL is planned but not yet started.

7. **Interactive REPL** — *not started*

   Planned to use Cranelift JIT for immediate execution.

8. **sash-expand** — *designed, not yet built*

   A safe source-level replacement tool. Designed but implementation
   has not begun.

---------------------------------------------------------------------

## Key Components

1. **Bash Parsing** — ✅ *done*

   tree-sitter-bash provides a complete Bash grammar with lossless
   syntax trees and Rust bindings.

2. **SSA IR** — ✅ *done*

   - 14 statement types
   - 10 expression types
   - 4 terminator types (branch, jump, return, exit)
   - Basic blocks with explicit control flow

3. **Janet IR Emission** — ✅ *done*

   Example mapping:

   ```
   Bash:  VAR=value
   SSA:   var1 = "value"
   Janet: (set var1 "value")

   Bash:  if cond; then body; fi
   Janet: (if cond body)

   Bash:  cmd1 | cmd2
   Janet: (pipe "cmd1" "cmd2")
   ```

4. **Cranelift Lowering** — ✅ *done*

   SSA IR → Cranelift IR → static ELF binary. Null-terminated counted
   strings (Decision #12). Linked with `ld` against `libsash_rt.a`.

5. **Validator** — ✅ *done*

   7 rule files, 14 diagnostic codes. Covers dynamic execution,
   unquoted expansion, backtick substitution, single-bracket tests,
   indirect expansion, alias restriction, and unsupported constructs.

6. **sash-rt** — ✅ *done*

   Pure Rust `#![no_std]` runtime. System calls via rustix. No libc,
   no C code, no inline assembly in safebash. Provides `_start`,
   string I/O, process exec, and exit.

---------------------------------------------------------------------

## Architecture

```
[Bash subset source]
        |
        v
[tree-sitter-bash parse]                     ✅ done
        |
        v
[Validator: 7 rules, 14 diagnostics]         ✅ done
        |
        v
[IR: 14 stmt types, 10 expr types]           ✅ done
        |
        v
[SSA: versioned vars, phi nodes]              ✅ done
   |            |
   |            +--> [Janet IR → correctness oracle]    ✅ done
   |
   +--> [Cranelift IR lowering]               ✅ done
             |             |
             |             +--> [JIT Execute in REPL]   planned
             |
             +--> [AOT → static ELF]         ✅ done
                    |
                    +--> [Sign → verify]      planned
```

---------------------------------------------------------------------

## Development Milestones

| Phase | Scope | Status |
|-------|-------|--------|
| 1. Parse and Validate | tree-sitter front-end, safe subset validation | ✅ done (exp 001–005) |
| 2. Validator Framework | rule infrastructure, ShellCheck integration | ✅ done (exp 006–011) |
| 3. IR and SSA | CST→IR lowering, SSA transformation, printer | ✅ done (exp 012–015) |
| 4. SSA Transform | versioned variables, phi nodes, basic blocks | ✅ done (exp 016) |
| 5. Janet Oracle | Bash→Janet→verify correctness tests | ✅ done (exp 017) |
| 6. Subshell Elision | optimize away unnecessary subshells | ✅ done (exp 018) |
| 7. Adversarial Audit | 5 models, 3 passes, security review | ✅ done (run-1) |
| 8. Audit Remediation | critical bypass fixes, high-gap fixes | ✅ done (exp 019–020) |
| 9. Cranelift Backend | hello → strings → exec → control flow → end-to-end | ✅ done (exp 021–026) |
| 10. Binary Signing | sign AOT artifacts, IPE/fs-verity integration | planned |
| 11. JIT + REPL | Cranelift JIT, interactive mode | planned |

---------------------------------------------------------------------

## Motivation

Bash is ubiquitous but:
- difficult to analyze statically
- unsafe by default
- full of edge cases and ambiguous behavior
- unable to participate in integrity and signing models

This project preserves the ergonomics of Bash while gaining:
- safety and predictability
- static analyzability
- verifiable, signed execution
- a chain of trust from source to runtime

This is an experimental compiler. It is not production-ready.

---------------------------------------------------------------------

## License

Private — not yet licensed.
