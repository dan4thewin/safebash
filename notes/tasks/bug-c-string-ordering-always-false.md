---
title: "`[[ a < b ]]` / `[[ a > b ]]` silently evaluate false in the C backend"
status: ready
priority: P1
kind: bug
depends: []
gates:
  - "`[[ abc < abd ]]` and `[[ abd > abc ]]` are true in c, matching native and bash"
  - "the reverse (`[[ abd < abc ]]`) is false in both backends"
  - "spec tests cover both operators in both directions, with NO mode guard"
  - "`emit_test_binary_op`'s fall-through arm is an ICE, not a `/* TODO */` comment returning \"0\""
---

# `<` and `>` string comparison is a silent wrong answer in c

Measured 2026-09-13 against the live binary:

```sh
if [[ abc < abd ]]; then echo "lt:yes"; else echo "lt:no"; fi
if [[ abd > abc ]]; then echo "gt:yes"; else echo "gt:no"; fi
```

| backend | result |
|---------|--------|
| bash    | `lt:yes gt:yes` |
| native  | `lt:yes gt:yes` |
| **c**   | **`lt:no gt:no`** |

`emit_c.rs:2620` handles `=`/`==`, `!=` and `=~`. Everything else falls to

```rust
_ => {
    writeln!(e.out, "    /* TODO test binary op: {} */", op).unwrap();
    "0".to_string()
}
```

so `<` and `>` compile cleanly, emit a comment, and yield constant **false**.
No error, no warning, no diagnostic — the program runs and gives the wrong
answer.

## Why this is P1

This is the failure mode the c backend exists to prevent. c is the
**differential oracle**: the whole point is that two independent backends
agreeing is evidence of correctness. An oracle that silently answers `false`
for an operator the other backend implements is worse than no oracle, because
it converts a wrong answer into an apparent agreement wherever the native
result also happens to be false.

## Why no test caught it

There is **zero** spec coverage for `[[ str < str ]]` — a grep over `spec/*.sh`
finds only `convert.sh` lines where `<` appears inside an expected-output
string. The operator was implemented in native, never in c, and never pinned.

## Fix

Implement both operators via `sash_strcmp` (already externed and used by
`=`/`!=` on the adjacent lines) — `< 0` and `> 0` respectively. Then convert
the fall-through to an ICE so the next unimplemented operator is a loud
compile-time failure instead of a silent `false`.

Note the ordering semantics question: bash's `<`/`>` inside `[[ ]]` sort by
the current locale's collation, but sash is locale-free and `sash_strcmp` is a
byte compare. Byte ordering is the right choice here (it matches `LC_ALL=C`
bash and keeps the runtime locale-free) but it should be stated in the DD
rather than left implicit.
