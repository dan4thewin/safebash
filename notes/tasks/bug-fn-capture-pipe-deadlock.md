---
status: blocked
priority: P2
kind: bug
depends: []
blocked-on: "DG — the fix is a fork with a semantic consequence (see Options)."
---

# `$(f)` on a user function hangs forever when the function writes more
# than the pipe buffer

## Measured, on the live binary (2026-09-13)

```sh
fn big {
  /bin/cat f.txt
}
str x=$(big)
attest x [[ -n $x ]]
echo $(strlen $x)
```

| `f.txt` size | result            |
|-------------:|-------------------|
|        1 000 | `1000`            |
|       60 000 | `60000`           |
|       70 000 | **hangs forever** |

The threshold is the pipe buffer (64 KiB on Linux). Pre-existing — it
reproduces identically before the `sash_capture_drain` work, which is how
it was found.

## Why

Capturing a *function* is not like capturing an external command. There is
no second process:

1. `sash_capture_start` creates a pipe and points fd 1 at the write end.
2. **The function body runs in this same process**, writing to that pipe.
3. `sash_capture_end` reads the pipe.

Nobody drains the pipe while step 2 is writing, so once the function has
written a pipe buffer's worth, its next `write` blocks. The only code that
could unblock it is step 3, which cannot start until step 2 returns.
Classic self-deadlock.

External `$(cmd)` does not have this problem: the writer is a forked child
and the parent is free to read concurrently.

Note the deadlock *masks* a second defect on the same path. Before the
drain work, `sash_capture_end` read into a fixed 64 KiB buffer and would
have truncated — but a capture large enough to truncate hangs first, so
the truncation was unreachable. Routing it through `sash_capture_drain`
removed it anyway; `cap.6` guards the path at a payload under the buffer.

## Options for DG

**A. Fork for function capture too.** Run the body in a child, as external
capture already does. Uniform, and the deadlock disappears.
*Cost, and the reason this is not just an implementation detail:* a forked
body's **variable assignments would no longer be visible to the caller**.
Today `$(f)` runs in-process, so a function that both prints and mutates
state works. Bash has exactly this behaviour — a command substitution runs
in a subshell and its assignments are lost — so A is *bash-compatible*,
but it is a **language-surface change** for sash and would silently break
any program relying on the current behaviour. It needs a DD.

**B. Spill to a temp file instead of a pipe.** Point fd 1 at a temp file,
run the body, rewind and read. No capacity limit beyond the filesystem, no
fork, and assignment visibility is unchanged.
*Cost:* needs a temp path (and `$TMPDIR` is environment input, which
DD#107 has opinions about), plus cleanup on the abort paths. Slower for
the common small capture.

**C. Drain incrementally.** Keep the pipe, but have the runtime read
whatever is available as the body writes.
*Cost:* the body is straight-line generated code, not a coroutine; there
is no point to interleave a read without a second thread or process. This
is A wearing a different hat.

**D. Fail loudly at the boundary.** Detect the blocking write and abort
with a diagnostic instead of hanging.
*Cost:* not a fix, but it converts an infinite hang into an honest error
and is compatible with doing A or B later.

**Recommendation: B**, with **D** as the immediate stopgap if B is not
wanted now. B is the only option that removes the limit without changing
what `$(f)` means. A is the "correct" shell semantics and matches bash,
but that is precisely why it needs DG: it changes observable behaviour of
existing programs and deserves a DD rather than a bug fix.

## Gates

- `$(f)` where `f` writes 1 MB completes and returns all of it, or fails
  with a diagnostic — it does not hang.
- Whatever is chosen, `cap.6` grows a large-payload sibling.
- If A is chosen: a DD covering assignment visibility in `$(f)`, and specs
  pinning the new rule.

## Found

2026-09-13, verifying the `sash_capture_drain` fix. The large-capture
spec for the function path hung, and it reproduced on the pre-fix build.
