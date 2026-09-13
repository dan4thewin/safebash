# 275 — Four copies of one loop, disagreeing (sash_capture_drain)

The next item in DG's order: the two `sash-rt` capture bugs. They had
never been filed — they lived in a sentence in `notes/status.md`:

> `sash_capture`/`sash_capture_status`/`sash_capture_end` still truncate
> at 64 KiB and strip only one trailing newline (route through
> `sash_read_all`).

Two of the three claims held up. The interesting parts were what the note
got wrong and what it did not know about.

## "Truncate at 64 KiB" undersells it

The predicted failure was a short string. The actual one:

```
$ x=$(/bin/cat 100kb.txt)
./b: /bin/cat: command failed (exit 141)
```

141 is 128 + 13 — **SIGPIPE**. The reader stops at 64 KiB and closes the
pipe; the child is still writing; the child dies; DD#55 sees a non-zero
status and aborts the program. So the bug was never a quiet short read in
the external-command case. It was a hard failure attributed to the *user's
command*, with a diagnostic naming `/bin/cat` as the thing that went
wrong. The one blamed is the one part that behaved correctly.

Both backends, identically: native rc=141, c rc=141.

## The newline rule was simply wrong

```sh
$(printf 'a\n\n\n')    # bash: "a"   sash: "a\n\n"
```

Bash strips *every* trailing newline from a command substitution. Two of
the copies stripped one. This is a byte-for-byte divergence from bash in
something as ordinary as capturing a command, and it had specs on the
surrounding behaviour but none on this.

## Four copies, and the one that was right

Searching for `65536` found three in the runtime — and a fourth in the
*compiler*: `codegen/value_handlers.rs` emits its own inline read loop,
~45 lines of Cranelift IR building a 64 KiB buffer, a read loop, and a
single-newline strip. The status note listed three functions; the native
path that most programs actually take was not among them.

Of the four, `sash_capture_end` alone stripped newlines correctly — a
`while` where the others had an `if`. Four copies of one routine, and they
had drifted into disagreeing with each other as well as with bash. That
is the argument for the fix being a collapse rather than three patches:
patching them separately preserves the conditions that produced the drift.

They now all call one `sash_capture_drain(fd)`, built on `sash_read_all`
(heap-growing, `EINTR`-retrying, aborts on a real error instead of
reporting it as EOF — fixed in the heap work). The native codegen's 45
lines of IR became a four-line call.

## `sash_capture` was dead, and annotated to stay quiet

`sash_capture` — the first of the three the note named — is called by
nothing. It is declared as an import in native codegen, stored in
`ExternFuncs`, and never referenced. The field carried

```rust
#[allow(dead_code)]
pub(super) capture: cranelift_module::FuncId,
```

so the compiler had noticed and been told not to mention it. It is
superseded by `sash_capture_status`, which is the same function plus the
exit status the DD#55 abort needs. Deleted, along with its declaration and
the silenced field, and the slot reused for `sash_capture_drain`.

An `#[allow(dead_code)]` on a symbol that duplicates a live one is how a
broken copy survives long enough to be listed in a bug report as if it
mattered.

## A third bug, which the fix could not touch

The spec for large *function* capture hung. It reproduces on the pre-fix
build, so it is not a regression — it is a distinct defect the 64 KiB
reader had been hiding:

`$(f)` on a user function does not fork. `sash_capture_start` points fd 1
at a pipe, **the body runs in this same process**, and `sash_capture_end`
reads afterwards. Nothing drains the pipe while the body writes, so past
64 KiB the body's own `write` blocks forever, waiting for a reader that
cannot start until the body returns.

The truncation bug on that path was therefore *unreachable* — any capture
big enough to truncate deadlocks first. Fixing the reader was still right,
but it could not have been observed.

Filed as `bug-fn-capture-pipe-deadlock`, **blocked on DG**, because the
obvious fix is not obviously correct. Forking the body (option A) is what
bash does and kills the deadlock, but a forked body's variable
assignments stop being visible to the caller — a language-surface change
that deserves a DD, not a bug fix. Spilling to a temp file (B) removes the
limit without changing what `$(f)` means, and is the recommendation.

So `cap.6` is labelled in the spec for what it is: a **non-regression
guard**, not proof of a fix. It passes on both sides. Its payload is kept
under the pipe buffer because the larger case cannot run at all yet.
Writing "this test does not demonstrate the bug" into the spec is cheaper
than a later reader inferring coverage that was never there.

## Landed

`$(cat 100kb)` returns 100 000 bytes in both backends where it died with
exit 141. `$(printf 'a\n\n\n')` is `a`, matching bash. Interior newlines
untouched; an all-newline capture is the empty string.

Specs `cap.1`-`cap.6` in `spec/misc.sh`. Five of the six fail against the
stashed build; the sixth is the guard described above.

Sweep **4083 passing, 29 skipped, 0 failing**, zero flips.
