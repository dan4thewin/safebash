# 144 — DD#75 implementation, part 3: polymorphic `len` + `${#x}` Int sugar (P2, demo)

Part 3 of the DD#75 order (test-first, spec tests). Delivers the demo need
(`int n=${#a}`) and fixes the `len <string>` garbage.

## 3a — `len` type-dispatch

`try_len_intrinsic` dispatched on the `$` sigil (`len a` → arrlen, `len $a` →
strlen), so `len <string>` ran `arrlen`/`table_len` on a non-table → garbage
(`3856007167`). Now it dispatches on the operand's **type**
(`state.collections`): collection → `arrlen` (count), else → `strlen` (bytes).
Handles `@a`, bare, and `$var`. `len s`→5, `len a`→3, `len m`→2, `len @a`→3.

## 3b — `${#x}` Int + count

Two hooks:
- `type_check::parameter_expansion_type`: `strlen`/`arrlen` → `Int` (a length
  is an int). So `int n=${#s}` / `int n=${#a}` compile (were SB4030).
- `transform::parameter_expansion_to_value`: `${#collection}` (parses to
  `strlen`) lowers to `arrlen` — so `${#a}` on an array is the **count**, not
  the byte length of a scalar slot (was `0`). `${#s}` stays strlen.

`int n=${#s}`→5, `int n=${#a}`→3, `echo ${#a}`→4. Supersedes
`length-operator-int-typing`.

**Test-first**: `spec/types.sh` 75.1–75.6, red → green. Sweep: native 0
failing; c 13 / janet 36 == baseline (janet handles `${#}` typing too).

## Deferred (task marked partial)

- **Arithmetic**: `$(( (len a) + 1 ))` / `$(( ${#a} + 1 ))` still → `1`. The
  arith path doesn't evaluate the length. Owned by
  `intrinsics-lengths-in-arithmetic` (separate P2).
- **Undefined-`len` reject**: `len nope` → `strlen` of empty → `0`. **Safe**
  (the `len-nonarray` garbage/segfault is gone — everything is now strlen-safe
  or arrlen-on-a-real-table), but DD#75 §2 wants a hard compile error. Deferred
  to avoid late regression risk.
