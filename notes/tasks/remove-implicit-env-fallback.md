title: Remove implicit env-var fallback in codegen
status: ready
severity: security
depends: []
gates: [53.1, 53.2, 53.3]
example: lower_value (sash/src/transform.rs)
index_keyword: getenv
  - tests
  - commit

Codegen implicitly mirrors env vars into sash vars, bypassing the
taint/bless gate. DD#53 says env vars and sash vars are separate
namespaces — the only way to read an env var is `$(getenv NAME)` + `bless`.

## Changes

### codegen.rs — scan_value_for_vars (~line 4290)
Remove ALL_CAPS branches (~4296, ~4307, ~4355) that add uppercase var
names to `positional_refs`. Only positional params (`$1`, `$@`, `$#`,
`$0`, `$$`, `$?`) should be auto-initialized. Uppercase names are just
sash variables — empty if never assigned.

### codegen.rs — env var fallback (~line 1258)
Remove the `else` branch that calls `sash_getenv` for unrecognized
variables ("Environment variable: $HOME, $PATH, etc."). An uninitialized
sash variable is an empty string, not an implicit getenv.

### codegen.rs — post-source re-read loop (~lines 1821-1855)
Remove the loop that re-reads all uppercase env-backed variables via
`sash_getenv` after a `source` command. (The source handler itself is
removed by `source-rejection`, but this loop is the env-fallback
violation regardless.)

### Tests
Audit all codegen/e2e tests that use `$HOME`, `$USER`, `$PATH` etc.
implicitly. Rewrite to use `$(getenv NAME)` + `bless`, or remove if
they exist only to test the implicit fallback. Known: `source_kvp_file`
depends on the fallback.
