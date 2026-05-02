title: Implement mapfile -m for KEY=VALUE → map
status: ready
severity: feature
depends: []
gates:
  - tests
  - commit

`mapfile -m varname` reads KEY=VALUE lines from stdin into a map
variable. This is the structured-input half of the DD#54 source
replacement pipeline:

```bash
map config
sash-expand ./vars.conf | mapfile -m config
bless config [[ ... ]]
```

## Semantics

- Split each line on the first `=`: left is map key, right is value
- Skip blank lines and `#` comment lines (matches sash-expand output)
- Populate the named map variable with the resulting key-value pairs
- Read from stdin (or redirected fd, same as array mapfile)

## Changes needed

### lower.rs
Recognize `-m` flag on mapfile/readarray commands. Lower to a
distinct IR form or flag on the existing mapfile IR node.

### transform.rs
Handle `-m` flag — emit SSA that reads lines and populates a map
(AssocArray) instead of an IndexedArray.

### codegen.rs
Generate code to call a runtime function that reads KEY=VALUE lines
and returns key-value pairs, then populate the map via SashTable.

### sash-rt
New runtime function: `sash_mapfile_kvp(fd) -> (keys, vals, count)`
or similar. Reads lines, splits on `=`, returns structured data for
codegen to insert into a SashTable.

### Tests
- mapfile -m with simple KEY=VALUE input
- mapfile -m skips blank lines and comments
- mapfile -m with values containing `=` (split on first only)
- mapfile -m from pipe (the sash-expand pipeline)
- mapfile -m result is tainted (external input)
