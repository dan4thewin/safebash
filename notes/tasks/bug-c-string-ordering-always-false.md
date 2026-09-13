---
title: "`[[ a < b ]]` / `[[ a > b ]]` silently evaluate false in the C backend"
status: done
completed: 2026-09-13
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

## Completion (2026-09-13)

The gap was **five** operators, not two. Native's `lower_test_binary`
(`codegen/tests.rs`) implements `<`, `>`, `-nt`, `-ot`, `-ef`; c handled only
`=`/`==`/`!=`/`=~` and dropped all five through a fail-open arm returning
constant **false**. Implemented in c via the already-externed `sash_strcmp`
plus newly-externed `sash_file_newer` / `sash_file_same_dev` — the same
runtime comparators native uses, so the two backends cannot drift. The c
fall-through is now an ICE, matching native.

Specs `ft.37`–`ft.46` (`spec/filetest.sh`, plan 36 → 46). Fail-first on the
first eight: native passed all of them already, and c failed **exactly 6 of
8** — `ft.38` and `ft.44` are the negative cases, which a constant-`false`
arm answers correctly by accident. Predicting that 6 in advance is what
distinguishes a test that measures the fix from one that measures the
weather.

`ft.45`/`ft.46` were added afterwards to pin that `-nt` resolves at
**nanosecond** granularity (two files 1 ms apart inside the same whole
second still order, and not symmetrically) — verified three-way against
bash. A seconds-truncating implementation passes `ft.41` and fails those.

Byte ordering is now pinned in a DD, per DG: see the 2026-09-13 addendum to
`notes/decisions/DD62-test-expr-simplification.md`. `ft.40` (`[[ Z < a ]]`
→ yes) is the test that would fail under a collating locale.

**Standing lesson.** This bug survived the entire life of the project behind
"nobody writes that". `[[ a < b ]]` had no spec test in *any* backend, ever.
The absence of a spec test is not evidence of correctness — it is the
absence of evidence.
