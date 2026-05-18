title: Remove SsaInst::Export and sash_setenv — dead code
status: done
severity: cleanup
depends:
  - remove-implicit-env-fallback
gates:
  - tests
  - commit

`SsaInst::Export`, `sash_setenv`, and all supporting code implement
`export` semantics that the validator rejects (SB5001). Sash has no
mechanism to push sash var changes back to the env (DD#53). This is
dead code that should be removed.

## Changes

### ssa.rs (~line 72)
Remove `SsaInst::Export { target, value, name }` variant.

### ssa_printer.rs (~line 43)
Remove Export print arm.

### ssa_taint.rs (~lines 121, 171)
Remove Export taint check arms.

### transform.rs (~lines 250-253, 281-282)
Remove Export emit for `DeclKeyword::Export` and `-x` flag.

### codegen.rs — extern declaration
Remove `setenv` field from ExternFuncs (~line 97).
Remove `sash_setenv` signature and declaration (~lines 410-417).
Remove `setenv: setenv_func` initialization (~line 760).

### codegen.rs — Export codegen arm (~lines 2154-2169)
Remove the `SsaInst::Export` match arm that calls `sash_setenv`.

### codegen.rs — scan passes
Remove `SsaInst::Export` from scan_value_for_vars dispatch (~line 1135).
Remove Export from scan pass that collects var definitions (~line 918).

### ir.rs (~line 124)
Keep `DeclKeyword::Export` — the lowerer still parses it so the
validator can reject it with a clear message (SB5001).

### sash-rt/src/lib.rs
Remove `sash_setenv` function. (The `sash_source_kvp` function that
calls it is removed separately by `source-rejection`.)

### Tests
Remove `export_basic` and `export_visible_to_child` from
codegen_tests.rs — these bypass the validator to test dead code.
