title: "janet: command-substitution capture is broken (user fns exec'd, multi-statement bodies truncated)"
status: deferred
deferred-by: DD104
priority: P1
kind: bug
depends: []
gates:
  - "janet: `fn f() { echo x }; v=$(f); echo \"[$v]\"` prints `[x]` (currently crashes in os/spawn)"
  - "janet: `v=$(echo a; echo b); echo \"[$v]\"` prints `[a\\nb]` (currently `[a]`)"
  - "spec `functions.sh` ice.1, ice.2, ice.4, ice.5, ice.6 pass in janet mode"

## Found

2026-09-08, fixing the codegen ICE for a `$(f)` capture used as a `[[ ]]` test
operand (`bug-fn-capture-in-test-operand-ice`). The new specs pass in native and
c and fail in janet — but the janet failures are **not** about tests. Probing
the assignment form shows the same breakage:

```sh
fn f() { echo x }
v=$(f)
echo "[$v]"
```

native/c print `[x]`. janet **crashes**:

```
  in os/spawn [src/core/os.c] on line 1533
```

The emitter renders a user-function capture as an external spawn:

```janet
(os/spawn ["f"] :p {:out :pipe})
```

`f` is a *sash function*, not a program on PATH, so the spawn fails. In a test
operand the same emission appears inside the `if` condition.

A multi-statement body is separately wrong — it keeps only the first statement's
output:

```sh
v=$(echo a; echo b)   # native/c: "a\nb"   janet: "a"
```

## Cause

`emit_janet` renders from the **AST**, so it never sees the transform's capture
lifting (`lift_int_func_calls`, and now `lift_test_captures`) that the native and
c backends rely on. It needs its own resolution of a cmdsub head against the
known-function set, plus a body-capture form for multi-statement `$(…)`.

Related: `emit-janet-from-ssa` would remove this whole class by construction.

## Note

Pre-existing and independent of the ICE fix — janet never panicked, it just
emitted the wrong thing. The specs are left **failing** rather than `skip_chk`'d
so the gap stays visible, consistent with the other janet-only failures.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
