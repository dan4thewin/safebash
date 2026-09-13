---
status: ready
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

## Options

**A. Give the runtime a capture stack.** `sash_capture_start` pushes,
`sash_capture_end` pops; codegen stops carrying handles at all. Removes the
slots and the whole class of clobbering, and shrinks the SSA surface.
*Cost:* changes the ABI of both entry points, so the Cranelift signatures,
`ExternFuncs`, and `emit_c` all move together. Depth bound has to be picked
(fixed array vs. heap).

**B. One slot pair per capture site.** Nesting depth is known statically, so
allocate distinct Cranelift variables / distinct C locals per site.
*Cost:* codegen-only and the ABI is untouched, but it keeps the handles in
generated code, which is the thing that made this possible. C needs unique
names per site.

**C. Reject nested capture in `validate.rs`.** A diagnostic instead of a
runtime `read failed`.
*Cost:* not a fix; bash supports this. Only worth it as a stopgap if A/B are
deferred, and it would need a DD to say the subset excludes it.

**Recommendation: A.** The bug exists because generated code owns state that
is naturally the runtime's, and B leaves that ownership where it is. A also
makes `CaptureStart`/`CaptureEnd` nullary-ish, which removes the two-i64
return dance in three places.

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
