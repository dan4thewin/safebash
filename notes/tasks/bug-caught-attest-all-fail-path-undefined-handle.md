---
title: "SIGSEGV: a caught attest-all whose gate fails leaves the array handle undefined for code after the `if`"
status: open
priority: P1
kind: bug
depends: []
gates: []
---

# `sash_table_len` faults on the failing path of `if attest … in …`

Found 2026-09-30, while measuring
`bug-attest-in-branch-leaks-past-merge`. A memory-safety fault in **generated
code**, reachable from ordinary source that compiles without a diagnostic.

## Reproducer

```sash
argv a
if attest v in a [[ $v == IMPOSSIBLE ]]; then /bin/echo p; else /bin/echo f; fi
for v in @a; do /bin/echo "$v"; done
```

```
$ env P=x ./cr A
f
Segmentation fault (core dumped)
```

Backtrace:

```
Program received signal SIGSEGV
#0  0x0000000000402339 in sash_table_len ()
#1  0x0000000000401449 in main ()
```

## Narrowed

| shape | result |
|---|---|
| caught gate **fails**, with `else`, read after the `if` | **SIGSEGV** |
| caught gate **fails**, no `else`, read after the `if` | **SIGSEGV** |
| caught gate passes, read after the `if` | fine |
| caught gate, read **inside** the `then` branch only | fine |
| bare (uncaught) `attest v in a`, read after | fine |

So the trigger is precisely: **the failing path of a caught attest-all, joined
back with a subsequent read of the subject array.** The read faults in
`sash_table_len`, i.e. the array's runtime handle is not a valid pointer on
that path.

## Nondeterminism — do not be fooled by a clean run

The compiled binary is byte-identical across compiles (same sha256), but
whether it faults depends on process memory layout:

- run directly in one shell: observed 0/10 crashes
- same binary, fresh compile each iteration: 10/20
- same binary under `env P=x`: **5/5, and 3/3 on every repeat**

So it reads uninitialized/garbage memory whose value happens to be benign in
some layouts. `setarch -R` (ASLR off) did not make it deterministic either.
**Always reproduce under `env`**, and treat a clean direct run as meaningless.

This is also why it is invisible to the spec suite: a spec that happened to run
in a benign layout would pass.

## Likely cause

`attest v in a [[ … ]]` binds `v` over `a` and retags the subject clean on
pass (DD#24: `lower_if_bless` is "branch-on-test, retag-clean-on-pass"). The
retag presumably produces a new SSA value for the array handle in the passing
block. On the **failing** block no such value is produced, so the phi at the
merge has an undefined incoming edge, and the later `sash_table_len` reads it.

That is a hypothesis from the shape and the backtrace, not a confirmed
diagnosis — start at `lower_if_bless` and the phi construction for the merge
block.

## Reachability

Today this is reachable from safe source **only because**
`bug-attest-in-branch-leaks-past-merge` wrongly lets the post-`if` read compile
at all. Fixing that bug makes this crash unreachable from safe source — but it
should still be fixed on its own terms:

- an undefined phi edge is a codegen defect regardless of who can reach it;
- `--no-taint` / `--no-taint-cst` bypass the validator entirely (DD#82), so the
  crash stays reachable through them;
- a correct future feature could legitimately join those paths.

## Found by

Measuring DG's question about runtime branches. The taint bypass came first;
the segfault appeared while probing the caught-gate variant of it.
