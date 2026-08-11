# 143 — DD#75 implementation, part 2: `join arr SEP` + non-array reject (P1)

Part 2 of the DD#75 order (test-first, spec tests).

## `bug-join-explicit-separator-segfault` (P1) — resolved

The task premise was off. Investigation showed the intrinsic's form was
`join SEP arr` (separator first), and the *segfault* came from a **non-array
operand** in the array slot: `join a ","` parsed `,` as the array, and
`arrjoin` on a non-table segfaulted.

**DG decision (2026-08-11):** make the form **`join arr SEP`** — array first,
separator second (least-surprise, collection-first, consistent with `len`/
`keys`). Asked before implementing (the old order would have kept the trap).

**Two fixes:**
1. `transform/expr_handlers.rs::join_intrinsic` — swapped to `(arr, sep) =
   (args[0], args[1])`; extracted `strip_word_quotes` (also lowers the fn's
   complexity).
2. `type_check.rs::check_join_operand` — `join`'s first operand must be a
   declared array/map, else **SB4030** (no more segfault on a scalar/literal).
   Hooked into `check_command` and the cmdsub descent (`check_subscript_in_stmt`)
   so `$(join …)` inside args is caught. `join_operand_name` resolves `@a`/bare.

**Test-first:** `spec/types.sh` 33.19 (`join a ","`→`x,y,z`), 33.20 (`@a`,
multi-char sep), 33.21 (space), 33.22 (non-array → clean reject, never a
segfault). Red → green. Rust `sash_join_comma`/`sash_join_multi_char_sep`
updated to the new order.

**Sweep:** native 0 failing; c 13 / janet 36 == clean baseline.

**janet deferred:** janet's join only does `@a` space-join — bare-name `join a`
execs `/usr/bin/join`; a separator errors (`bad slot, expected string, got
array`). Guarded 33.19–33.21 to native,c; filed `janet-join-intrinsic-gaps`
under the janet effort.
