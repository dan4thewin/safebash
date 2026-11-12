title: "BUG (emit_c) — external `@`{a:?} splat after a preceding statement emits non-compiling C (jump into VLA scope)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`/usr/bin/touch z; array a=(x y); /usr/bin/printf \"[%s]\" @{a:?x}` compiles under --emit-c (gcc) and prints `[x][y]` (matches native)"
  - "no `jump into scope of identifier with variably modified type` from any `@{a:?}`/`@{a:-}` external splat"
  - "0 native spec regressions; c oracle re-greens for the affected forms"

# BUG — emit_c `@{a:?}` splat generates a goto that jumps into a VLA scope

## Repro (c backend only; native is correct)

```
$ printf '/usr/bin/touch z; array a=(x y); /usr/bin/printf "[%s]" @{a:?x}\n' > t.sash
$ sash --emit-c t.sash > t.c
$ gcc -nostartfiles -static -fno-stack-protector -o tb t.c libsash_rt.a -lpthread
t.c:86:29: error: jump into scope of identifier with variably modified type
$ sash -c '/usr/bin/touch z; array a=(x y); /usr/bin/printf "[%s]" @{a:?x}' -o tb && ./tb
[x][y]                                   # native: correct
```

Glob-independent: reproduces with a literal `array a=(x y)` (no `*`).
Discovered 2026-07-29 while writing DD#73 glob specs (the glob spec that
tripped it was rewritten to observe via for-in; this bug is unrelated to
DD#73 — emit_c was untouched by that work).

## Root cause (to confirm)

The external-command array-splat lowering in `emit_c.rs` (the `@{a:?}` /
`@{a:-}` disposition path — mirrors `codegen/commands.rs`
`resolve_splat_tables`) emits a **variable-length array** (the argv is
sized at runtime from the splat cardinality) together with a **`goto`** for
the `:?` abort / empty-disposition branch. In C, a `goto` that jumps from
before a VLA declaration into its scope is ill-formed ("jump into scope of
identifier with variably modified type"). A preceding statement (so the
jump originates in an outer scope) makes it manifest.

Likely fixes: hoist the VLA (or replace it with a heap alloc / fixed cap +
loop) so no `goto` crosses its declaration; or restructure the abort branch
to not jump across the VLA (compute the disposition before declaring the
argv array). Prefer matching the native lowering's control-flow shape.

## Scope

c differential-oracle backend only; native and janet unaffected. Add a
c-mode spec (e.g. under glob_splat.sh or a new emit_c regression) once
fixed. Until then, DD#73's `glob.sh` g.3 observes array-glob elements via
for-in rather than an external splat, so it does not depend on this bug.

## Re-measured 2026-09-13 — already done

The filed repro, verbatim:

```
$ SASH_MODE=c  /usr/bin/touch z; array a=(x y); /usr/bin/printf "[%s]" @{a:?x}
rc=0  out=[[x][y]]         # identical to native
```

gcc no longer reports "jump into scope of identifier with variably modified
type". Fixed by unrelated work at an unrecorded point.
