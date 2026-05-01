title: Architecture

## Pipeline

```
tree-sitter CST → validate (rules + taint) → lower (CST→IR, coercions)
→ type check → transform (IR→SSA) → fold → codegen (Cranelift) → link → static ELF
```

Also supports `--janet` backend and `--emit-source`/`--annotate`/`--extract` modes.

## Key source files (sash/src/)

- `main.rs`, `lib.rs` — entry points
- `lower.rs` — CST → IR lowering with coercions
- `ir.rs` — IR definitions
- `type_check.rs` — type checking pass
- `transform.rs` — IR → SSA transformation
- `ssa.rs`, `ssa_printer.rs` — SSA representation and printing
- `codegen.rs` — Cranelift code generation (~4500 lines)
- `taint.rs` — CST taint analysis (~850 lines)
- `ssa_taint.rs` — SSA taint verification backstop (~340 lines)
- `validator.rs`, `rule.rs` — validation framework
- `rules/` — individual validator rules
- `diagnostic.rs` — error reporting
- `emit_janet.rs` — Janet backend
- `emit_source.rs`, `printer.rs` — source emission modes
- `known_externals.rs` — allowlisted external commands

## Taint model — defense in depth (DD#51)

1. **CST pass** (`taint.rs`) — primary enforcer, runs before lowering,
   gives source-precise diagnostics (SB4020)
2. **SSA pass** (`ssa_taint.rs`) — backstop verifier, runs after SSA
   construction, emits ICE if anything tainted reached codegen

If the SSA pass fires, that's a bug in the CST pass.

## Runtime (sash-rt/)

`#![no_std]` Rust runtime with rustix for syscalls. No libc.
FreeRTOS heap_4 allocator. Unified SashTable for arrays/maps.
Plan 9 regex engine (C). Thin unsafe boundary at extern C entry
points, ~80% safe Rust.

## Tree-sitter fork

Custom grammar at `~/src/tree-sitter-bash-safebash` with extensions:
`int`, `const`, `array`, `map`, `bless`, `fn`, `${var:s/re/str/}`.

## Build system

Plan 9 `mk` (not make). Top-level `mkfile` with `build`, `test`, `clean`.
