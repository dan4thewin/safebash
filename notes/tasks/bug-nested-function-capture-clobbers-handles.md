---
status: done
priority: P2
kind: bug
depends: []
---

# Nested function capture clobbers the outer capture's handles

## Measured, on the live binary (2026-09-13)

```sh
fn g { echo inner; }
fn h {
  str i=$(g)
  attest i [[ -n $i ]]
  echo "outer-$i"
}
str o=$(h)
attest o [[ -n $o ]]
echo "[$o]"
```

| case                                  | result                      |
|---------------------------------------|-----------------------------|
| `h` called directly (one capture)     | `outer-inner`               |
| `o=$(h)` (capture inside a capture)   | `sash: read failed`, rc 1   |
| bash                                  | `[outer-inner]`             |

**Pre-existing.** It reproduces identically on the pipe-based runtime, so it
predates the `memfd` change (journal 276) and the `sash_capture_drain`
collapse (journal 275). Both modes.

## Why

The capture handles live in **a single pair of slots**, not a stack:

- native — `capture_saved_var` / `capture_pipe_var`, allocated once per
  function in `codegen/entry.rs:194` and read back in
  `codegen/dispatch.rs:1105`.
- c — `i64 __cap_saved = 0; i64 __cap_pipe = -1;` emitted once per program
  in `emit_c.rs:381`.

`CaptureStart` writes both (`dispatch.rs:110`); `CaptureEnd` reads both.
With two captures live at once the inner `CaptureStart` overwrites the
outer's pair, and the inner `CaptureEnd` closes what it finds. The outer
`CaptureEnd` then restores an already-closed fd and drains a closed handle —
hence the generic `read failed` rather than anything that names the cause.

Nothing rejects this at compile time, and there is no DD covering it, so it
is a gap rather than a deliberate restriction.

## It is a sequencing problem, not an architectural one

DG's read, and it is correct: **sash inlines function bodies**, so capture
nesting depth is known at compile time. The validator says so out loud —

```
SB5037: recursive call to 'down' is not supported — sash inlines functions,
so only a direct self tail-call (`return down …`) is allowed (it lowers to
a loop); rewrite other recursion as a loop
```

Recursion is rejected, and the one permitted form (direct self tail-call)
lowers to a *loop*, which re-enters a capture site sequentially rather than
nesting it. There is no construct that produces a dynamic capture depth.

That kills the case for a runtime-owned stack. A stack exists to handle a
depth you cannot know; here you can always know it. Codegen already emits
the whole program as one Cranelift function with all bodies inlined as
blocks (`entry.rs:119` allocates one variable set for the lot) — so the
inner and outer captures are plainly visible in one instruction stream, and
the only defect is that they were handed the same two variables.

**Fix: one slot pair per capture site.** Walk the SSA, number the capture
sites, allocate `capture_saved_var_N` / `capture_handle_var_N` per site in
`setup::allocate_variables`, and carry the index on `SsaInst::CaptureStart`
/ `CaptureEnd` so each end pairs with its own start. In C, the same index
names `__cap_saved_N` / `__cap_handle_N`. No ABI change, no runtime change,
no new concept.

Equivalently and more cheaply, the slots could be saved/restored around an
inner capture, since an inner pair's lifetime is strictly contained in the
outer's — but per-site slots need no ordering argument to be correct, so
they are the better version of the same idea.

### Rejected: a runtime capture stack

The first draft of this note recommended moving the handles into a runtime
stack, on the reasoning that "generated code owns state that is naturally
the runtime's". That argument does not survive the inlining fact: the state
is static, and a stack would pay an ABI change across Cranelift signatures,
`ExternFuncs`, and `emit_c` to dynamically track something the compiler
already knows. Recorded so the reasoning is not re-derived.

### Rejected: reject nested capture in `validate.rs`

A diagnostic instead of `read failed`. Not a fix, and not needed once the
above is a slot-numbering exercise.

## Gates

- The reproduction above prints `[outer-inner]` in both modes.
- A three-deep nest works.
- An abort inside an inner capture still restores the outer's stdout.
- `cap.6`–`cap.12` unchanged.

## Found

2026-09-13, while writing `cap.10` (a redirect inside a captured body).
That test happened to nest two captures for unrelated reasons and failed;
the pre-fix build failed identically, which is what identified it as
pre-existing rather than a regression.

## Fixed 2026-09-13

Per-site slots, as DG read it. `SsaInst::CaptureStart`/`CaptureEnd` carry a
`site: u32`, handed out by `VersionState::alloc_capture_site()` at each of
the two lowering points (`capture_func_into`, `capture_body_into`) *before*
the body is lowered, so a nested capture inside the body gets a different
number.

Native: `FuncState.capture_slots` is a `Vec<(Variable, Variable)>` sized by
`SsaProgram::capture_site_count()`, threaded to `InstCtx` as a slice.
C: `__cap_saved_N` / `__cap_handle_N` per site. No ABI change, no runtime
change — `sash_capture_start`/`end` are untouched.

Specs `cap.13` (two deep) and `cap.14` (three deep) verified failing before
the change. `cap.15`/`cap.16` are guards that pass on both sides;
`cap.15` also records an asymmetry worth remembering — `$(cmd)` on an
*external* never uses `CaptureStart`/`CaptureEnd` at all (it forks and
drains via `emit_cmdsub_exec`), so it never consumed a site and could not
collide.

Sweep 4103 passing, 29 skipped, 0 failing, zero flips, +8 specs.

Worth noting the limitation was **documented but never filed**. The C
emitter carried the comment "No nesting (single capture-state pair),
matching native's single capture_saved/pipe vars" — an accurate description
of a bug, sitting in the source, read as a design note.
