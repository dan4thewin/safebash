title: Bash-mode positional coercion
status: ready
depends:
  - dd46-reject-toplevel-positionals
gates:
  - tests
  - commit

DD#46 remainder — top-level `$1..$9`, `$@`, `$*` should be coerced
to `$(argv N)` / `$(argv)` in bash-compatibility mode.

Note: sash-native rejection is a prerequisite — see
dd46-reject-toplevel-positionals task.
