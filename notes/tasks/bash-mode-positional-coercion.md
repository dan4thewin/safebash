title: Bash-mode positional coercion
status: ready
depends: []
gates:
  - tests
  - commit

DD#46 remainder — top-level `$1..$9`, `$@`, `$*` should be coerced
to `$(argv N)` / `$(argv)` in bash-compatibility mode. Sash-native
mode already rejects them.
