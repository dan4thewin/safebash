# Experiment 002: tree-sitter dependency — adding the parser

**Goal:** Add `tree-sitter` and `tree-sitter-bash` crate dependencies.
Compile successfully. Verify the Bash grammar loads.

**Actions:**
- Added to Cargo.toml: `tree-sitter = "0.25"`, `tree-sitter-bash = "0.23"`
- Changed main to create a Parser, load the Bash language, print confirmation
- Test: create parser + load language without panic

**Results:**
- 25 crates downloaded, including `cc` (C compiler driver for the
  tree-sitter C code)
- Build time: ~10s (clean, compiling tree-sitter C sources)
- Grammar loads successfully
- tree-sitter 0.25.10, tree-sitter-bash 0.23.3

**Conclusion:** tree-sitter-bash works via FFI. The C dependency is
invisible at the API level — Rust types all the way. The `cc` crate
handles finding the system C compiler automatically.

---

