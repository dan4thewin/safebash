---
title: "SIGSEGV: a caught attest-all whose gate fails leaves the array handle undefined for code after the `if`"
status: done
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

---

## Resolved 2026-09-30

Fixed, and the diagnosis above was **wrong in one important way**.

### The nondeterminism is at compile time, not run time

This filing says the binary is byte-identical across compiles and that the
fault "depends on process memory layout". It does not. Compiling one unchanged
source ten times produced **two different binaries**:

```
compile 1: sha=7c6a03c943fe rc=139     compile 2: sha=131fb478a1f9 rc=0
compile 3: sha=7c6a03c943fe rc=139     compile 4: sha=131fb478a1f9 rc=0
```

Each binary is deterministic; *which one you get* was not. That also explains
the observation that dressed itself up as layout sensitivity — `env P=x` did
not perturb the runtime, it just happened to accompany a bad compile. The
`then/fi` and `{ }` spellings of the same program "differing" was the same
illusion: two compiles, two outcomes.

Non-reproducible codegen is a defect in its own right and was fixed as one.

### Root cause

`codegen::types::find_array_slot` resolves a collection by **name**, and when
`a_0` is absent it took `vars.keys().find(|k| k.starts_with("a_"))` — an
arbitrary `HashMap` key.

The only thing that ever puts a *second* `a_*` key in that map is
`emit_attest_retag`. It implemented DD#61's clean-retag by
`declare_var`/`def_var`-ing a fresh pointer pair — and it runs in the **pass
block only**, because that is where `transform/control_flow.rs` emits
`AttestRetag` for `AttestKind::Coll`. So on the failing path `a_2` existed as a
key but was never defined, and if the scan happened to return it,
`sash_table_len` got an undefined pointer.

Confirmed by the differential oracle: `--emit-c` never faulted, because
`emit_c.rs` has always treated `AttestRetag` as a no-op ("versions collapse to
one slot"). Native was the only backend that materialised it.

The filing's "undefined phi incoming edge" guess was close in spirit but wrong
in mechanism — there is no phi here at all. `lower_if_attest` deliberately
restores the pre-state at the merge, and that is correct: the retag is a
compile-time clean-tag valid only on the passing path.

### Fix

Two changes, either of which stops the crash; both landed, because they close
different halves.

1. **`emit_attest_retag` aliases instead of copying** — the target version maps
   to the *same* `VarSlot`. The retag changes nothing at runtime, so it must
   not create a runtime value. This matches the c backend.
2. **`find_array_slot`'s fallback is deterministic** — the **lowest** version
   present, which generalises the existing `_0`-first rule: the base version is
   defined on every path by construction, so a version introduced on only some
   path can never shadow it. The suffix must parse as a number, so a variable
   named `a_b` can no longer masquerade as a version of `a`.

Ten compiles of the reproducer now yield one binary, correct output, rc 0.

### Pinned by

`spec/taint_switch.sh` `sw.12` (fail path) and `sw.13` (pass path), under
`--no-taint` — DD#121 rejects this source outright, so the bypass is the only
route left to it, which is precisely why it is worth pinning.

`sw.12` **compiles six times on purpose**. Two details make a naive spec
useless here, both found by measuring against the reverted fix:

- The read must be a **splat** (`for v in @a`). `len a` and `join a` do not go
  through `find_array_slot` and never reproduced it — 0/12.
- One compile only caught it ~60% of the time, so the loop is what makes the
  gate real. Verified: the pin fails 3/3 sweeps against the original code.
