title: "DD#75 §2 — len is type-polymorphic (array/map → count, string → bytes); ${#x} is Int sugar for $(len x)"
status: done
priority: P2
kind: task
depends: []
resolved: "2026-08-20 — core done: len type-dispatch (array/map→count, string→bytes, no `len <string>` garbage) + ${#x} Int-typed sugar. Arithmetic (`$(( (len a)+1 ))`) done under intrinsics-lengths-in-arithmetic. The undefined-`len` hard reject is SAFE as-is (no garbage/segfault) and split into `len-undefined-reject` (needs loop-var/param scope tracking to avoid false positives — confirmed check_for doesn't register the loop var)."
gates:
  - "[done] len <array|map> → count; len <string> → byte length; dispatch on TYPE (state.collections), not the `$` sigil (fixes the `len <string>` garbage)"
  - "[done] ${#x} is Int-typed sugar: int n=${#s} and int n=${#a} compile (was SB4030); ${#a} on an array yields the count (was 0)"
  - "[deferred → intrinsics-lengths-in-arithmetic] $(( (len a) + 1 )) and $(( ${#a} + 1 )) evaluate the count (still 1 — arith path unresolved)"
  - "[deferred] len <undefined> → compile error (today: strlen of empty → 0; SAFE — no garbage/segfault — but not yet a hard reject)"
  - "[done] 0 native/c/janet spec regressions"

# DD#75 §2 — polymorphic `len`; `${#x}` Int sugar

## Resolution (2026-08-11, partial)

Shipped the core (test-first, `spec/types.sh` 75.1–75.6):

- **`len` type-dispatch** (`transform/expr_handlers.rs::try_len_intrinsic`):
  a collection → `arrlen` (count), else → `strlen` (bytes). Handles `@a`,
  bare, and `$var`. Retires the `len a` vs `len $a` syntactic split and fixes
  the `len <string>` garbage (was `arrlen`/`table_len` on a non-table).
- **`${#x}` Int + count** (`type_check.rs::parameter_expansion_type` →
  `strlen`/`arrlen` are `Int`; `transform/expressions.rs::
  parameter_expansion_to_value` → `${#collection}` lowers to `arrlen`). So
  `int n=${#s}` (5) and `int n=${#a}` (3) compile, and `${#a}` renders the
  count. **Supersedes** `length-operator-int-typing`.

## Remaining

- **Arithmetic** (`$(( (len a) + 1 ))`, `$(( ${#a} + 1 ))` → still `1`): the
  arith path doesn't evaluate the intrinsic/length. Owned by
  `intrinsics-lengths-in-arithmetic`.
- **Undefined-`len` hard reject**: `len nope` compiles to `strlen` of an
  empty var → `0`. This is **safe** (no garbage/segfault — the
  memory-safety concern from `len-nonarray-operand-reject` is resolved by
  the dispatch), but DD#75 §2 wants a compile error. Add a
  `check_len_operand` reject (mirror `check_join_operand`) — deferred to
  avoid late scope/regression risk (loop-var / param false-positive care).

