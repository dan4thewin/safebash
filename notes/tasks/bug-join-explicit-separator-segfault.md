---
title: "P1 memory-safety: join <arr> <sep> (explicit separator) segfaults"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "$(join a \",\") — join with an explicit separator — produces the joined string, never segfaults"
  - "$(join a) (default space separator) still works"
  - "0 native/c/janet spec regressions; add a spec pinning explicit-separator join"
---

# `join` explicit separator + non-array operand (RESOLVED)

## Resolution (2026-08-11)

The task premise was slightly off. The real picture, once investigated:

- The intrinsic's form was `join SEP arr` (separator **first**). DG decided
  (2026-08-11) to make it **`join arr SEP`** (array first, separator second —
  least-surprise, collection-first, consistent with `len`/`keys`). Swapped in
  `transform/expr_handlers.rs::join_intrinsic`.
- The **segfault** was a *non-array operand* in the array slot (`join a ","`
  under the old order parsed `,` as the array; `arrjoin` on a non-table →
  segfault). Fixed by a compile-time reject in `type_check.rs`
  (`check_join_operand`): `join`'s first operand must be a declared array/map,
  else **SB4030** ("`join` needs an array or map as its first operand …").

`join "," @a` (old sep-first) is no longer used; `join @a ","` / `join a ","`
(new order) → `x,y,z`. `join a` (space) unchanged.

**Test-first**: `spec/types.sh` 33.19–33.22 (join arr sep, multi-char sep,
default space, non-array reject). Rust `sash_join_comma`/`sash_join_multi_char_sep`
updated to the new order. native 0 failing; c/janet at baseline.

**janet deferred**: janet's join only handles `@a` space-join — bare-name
`join a` execs `/usr/bin/join`, and a separator errors. Guarded 33.19–33.21 to
native,c and filed under the janet effort (`janet-join-intrinsic-gaps`).

