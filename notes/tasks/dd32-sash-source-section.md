title: ELF .sash_source section and --extract
status: done
severity: feature
gates: [32.1, 32.2]
example: sash/src/codegen.rs:compile_ssa
index_keyword: sash_source
---

DD#32: compiled binary contains .sash_source ELF section,
sash --extract dumps it.
