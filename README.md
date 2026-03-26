# Safe Bash Subset Compiler (Rust + Cranelift + SSA + Janet)

Shell scripts run in critical paths — deployment, provisioning, system
updates — but they cannot participate in a chain of trust. They cannot
be signed, verified, or refused by the kernel when tampered with. This
project changes that.

This is a compiler for a safe, statically analyzable subset of Bash,
implemented in Rust, that produces signed native artifacts whose
integrity can be enforced at execution time.

The chain of trust:

    Author writes safe-subset script
        → Compiler produces deterministic native binary
            → Binary is signed in the build pipeline
                → System policy (IPE / fs-verity) verifies before execution

Unsigned or tampered artifacts are refused. The script's behavior is
visible in the source. The binary's integrity is guaranteed at runtime.

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

1. **Security and Signing**

   Compiled safe Bash artifacts:
   - are produced as AOT-compiled native binaries
   - are signed as part of the build pipeline
   - are verified at load or execution time by system policy

   The intent is to integrate with mechanisms like:
   - SELinux IPE (Integrity Policy Enforcement)
   - fs-verity or similar file integrity schemes

   Exact details of the signing scheme, key management, and policy
   integration are to be designed, but verifiable execution is a
   first-class goal from the start.

2. **Safe Subset of Bash**

   A restricted, well-specified subset that is:
   - predictable
   - statically analyzable
   - portable
   - suitable for compilation and verification

   The safe subset is informed by ShellCheck's ~500 documented rules
   (https://www.shellcheck.net/wiki/). Constructs that ShellCheck
   flags as dangerous or error-prone are rejected by the safe subset.

   Two modes:
   - **Strict mode** — reject scripts that use unsafe constructs.
     The compiler reports what needs to change and why.
   - **Coerce mode** — automatically transform unsafe constructs
     into their safe equivalents where possible (e.g., unquoted
     `$var` → `"$var"`, backtick substitution → `$()`). Error
     only for constructs with no safe counterpart.

3. **Rust Front-End**

   The compiler front-end:
   - Parses Bash syntax using tree-sitter-bash
   - Validates against the safe subset
   - Produces a structured IR
   - Converts IR into SSA form

4. **SSA (Static Single Assignment)**

   SSA ensures:
   - every variable is assigned exactly once
   - all variable references are explicit
   - no shadowing or ambiguous lookup
   - control flow merges are explicit via phi nodes

   SSA eliminates scoping ambiguity entirely. All bindings are explicit
   and all data flow is traceable.

5. **Janet as a Standalone Executable Correctness Oracle**

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

   The Janet output is not a "program" in the traditional sense.
   It is a serialized execution plan that Janet can run.

6. **Cranelift Backend (JIT and AOT)**

   Cranelift provides:
   - fast JIT compilation for the REPL
   - portable machine code generation for AOT
   - a natural fit for incremental compilation

   The SSA IR is lowered into Cranelift IR for:
   - JIT execution in the REPL
   - AOT compilation to signed native binaries

7. **Interactive REPL**

   The REPL:
   - accepts Bash-subset input
   - parses → validates → IR → SSA → compiles → runs
   - uses Cranelift JIT for immediate execution
   - optionally displays the Janet IR
   - supports incremental definitions

   The REPL is a development and debugging tool.
   Production use relies on AOT-compiled, signed binaries.

---------------------------------------------------------------------

## Key Components

1. **Bash Parsing**

   tree-sitter-bash provides:
   - a complete Bash grammar
   - lossless syntax trees
   - Rust bindings

2. **SSA IR**

   The SSA IR sits between parsing and code generation. It must:
   - represent control flow explicitly
   - model pipelines, redirections, and commands
   - map cleanly to both Janet and Cranelift
   - eliminate scoping ambiguity through single assignment

3. **Janet IR Emission**

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

4. **Cranelift Lowering**

   The SSA IR is lowered into Cranelift IR, enabling:
   - JIT execution for the REPL
   - AOT compilation to native code
   - integration with signing and verification workflows

---------------------------------------------------------------------

## Planned Architecture

```
[Bash subset source]
        |
        v
[tree-sitter-bash AST]
        |
        v
[Validator: safe subset checks]
        |
        v
[IR]
        |
        v
[SSA Transformation]
   |            |
   |            +--> [Emit Janet IR → correctness oracle]
   |
   +--> [Lower to Cranelift IR]
             |             |
             |             +--> [JIT Execute in REPL]
             |
             +--> [AOT Compile → Signed Artifact → System Verification]
```

---------------------------------------------------------------------

## Development Milestones

Each layer is independently testable before the next is built.

1. **Parse and Validate.** tree-sitter-bash front-end with safe subset
   validation. Input: Bash source. Output: accept/reject with
   diagnostics. Testable with a corpus of valid and invalid scripts.

2. **IR and SSA.** Structured intermediate representation with SSA
   transformation. Testable by inspecting IR output for known inputs.

3. **Janet Oracle.** Emit Janet from SSA IR. Run Janet, capture output.
   Property-based tests: for any safe-subset script, the Janet oracle
   and the eventual compiled binary must produce identical output.

4. **Cranelift Backend.** Lower SSA IR to Cranelift. JIT for the REPL,
   AOT for deployment. Testable against the Janet oracle.

5. **Signing and Verification.** Sign AOT artifacts. Integrate with
   IPE / fs-verity. Testable by attempting to run unsigned or tampered
   binaries under an enforcing policy.

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

---------------------------------------------------------------------

## License

TBD.
