# safebash — Experiment Journal

## Prologue

Shell scripts run in critical paths — deployment, provisioning,
system updates — but they cannot participate in a chain of trust.
They cannot be signed, verified, or refused by the kernel when
tampered with. safebash changes that.

This journal documents the incremental journey of building a compiler
for a safe subset of Bash. Rather than attempt a monolithic build of
the full pipeline (parser → validator → IR → SSA → Janet oracle →
Cranelift → signing), we proceed through a series of small,
self-contained experiments. Each experiment produces a working
artifact that demonstrates one concept needed for the compiler.

### How this work is being done

safebash is a collaboration between DG (the human directing the
project) and an AI — specifically, Claude (Anthropic) running as
GitHub Copilot in the terminal. The AI writes the code, the Rust,
and these documents. DG directs the effort: setting goals, reviewing
output, correcting mistakes, and making design decisions.

The workflow is conversational. DG describes what should happen next.
The AI proposes an approach, writes the code, tests it, and produces
a working experiment. DG reviews the result, points out what's wrong
or what was misunderstood, and the AI corrects course.

The prose in this journal and the companion guide is AI-generated.
Direct quotes from DG are attributed. The goal is transparency: a
future reader should know that an AI wrote these words, that a human
directed the work, and that neither could have done it alone in quite
this way.

### Design decisions (established in planning)

- **Rust** implementation — compiler, front-end, Cranelift backend
- **tree-sitter-bash** for parsing
- **SSA IR** between parsing and code generation
- **Janet** as external correctness oracle (S-expression syntax,
  built-in POSIX primitives, lexically scoped, independent
  verification of compiler semantics)
- **Cranelift** for JIT (REPL) and AOT (deployment) compilation
- **Signing + IPE/fs-verity** integration for tamper-evident execution
- **`sash`** as the CLI binary name

---

