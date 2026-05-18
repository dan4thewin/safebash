title: Remove command -v runtime implementation
status: done
severity: cleanup
depends: []
gates: [23.3]
example: sash/src/rules/process_substitution.rs:check
index_keyword: command_v
  - tests
  - commit

`sash-rt/src/lib.rs` (~line 1770) implements `command -v` with
runtime PATH search. DD#23 says `command` does not exist in
sash-native — it's a coercion target only (bash `command -v foo`
→ sash `[[ -x /resolved/path ]]`).

Remove the `command -v` / `sash_command_v` runtime implementation.
