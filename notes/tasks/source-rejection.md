title: Source rejection — SB4002 becomes hard error
status: done
severity: design
depends:
  - mapfile-map-mode
  - remove-export-ssa-dead-code
gates: [54.1a, 54.1b]
example: sash/src/rules/dynamic_execution.rs:check
index_keyword: source_kvp
  - tests
  - commit

DD#54 supersedes the source/. coercion. `source` and `.` are now hard
errors (SB4002). The replacement is an explicit pipeline:

```bash
map config
sash-expand ./vars.conf | mapfile -m config
bless config [[ ... ]]
```

## Changes

### rules/dynamic_execution.rs (~line 43)
Change SB4002 from `Diagnostic::marker` to `Diagnostic::error`.
Update message: "'source'/'.'' is not allowed — use
`sash-expand file | mapfile -m var` instead".

### codegen.rs (~lines 1812-1855)
Remove the entire `source`/`.` command handler (the block matching
`name == "." || name == "source"`).

### codegen.rs — source_kvp extern
Remove `source_kvp` field from ExternFuncs (~line 113).
Remove `sash_source_kvp` signature and declaration (~lines 557-563).
Remove `source_kvp: source_kvp_func` initialization (~line 776).

### sash-rt/src/lib.rs (~line 808)
Remove `sash_source_kvp` function entirely.

### emit_janet.rs
Check for source/. handling in the Janet backend. Remove if present.

### SUBSET.md
Document that source/. are rejected (SB4002). Show the replacement
pipeline with sash-expand + mapfile -m + bless.

### Tests
Remove `source_kvp_file` from codegen_tests.rs.
Add e2e test confirming SB4002 is a hard error for both `source`
and `.` commands.
