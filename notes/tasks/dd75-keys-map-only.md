title: "DD#75 §3 — keys is map-only (indexed/scalar → compile error)"
status: done
priority: P3
kind: task
depends: []
gates:
  - "$(keys <map>) → splat of the string keys (unchanged); keys <indexed array> and keys <scalar> → compile error pointing at `for (( i=0; i<$(len a); i++ ))` / @a"
  - "the indexed-keys blank-output bug is gone (replaced by the reject)"
  - "map values via @m and ${m[$k]} still work; no `values` intrinsic added"
  - "0 native/c/janet spec regressions; specs pin keys-on-map OK, keys-on-array/scalar rejected"

## Resolution (2026-08-14, unattended)

Added `check_keys_operand` (`type_check.rs`, sibling of `check_join_operand`):
`keys`'s operand must be a declared **map** (`assoc_arrays`), else SB4030 —
an indexed array (message points at `for (( i=0; i<$(len a); i++ ))` / `@a`)
or a scalar/undefined. Hooked into `check_command` and the cmdsub descent.
Also added a **For arm** to `check_stmt` (was missing) so for-in *header*
words are type-checked — `for k in $(keys a)` now reaches the reject (the
header is expressions, not a child body). Fixes the indexed-keys blank/null
output by rejecting at compile time.

Test-first: `spec/types.sh` 75.7 (map keys still work — guarded native,c: janet
lacks the keys intrinsic, execs it, pre-existing cf. janet:33.10), 75.8
(indexed → reject), 75.9 (scalar → reject). native 0 failing; c 13 / janet 36
== baseline. janet keys gap noted in `janet-join-intrinsic-gaps`.

