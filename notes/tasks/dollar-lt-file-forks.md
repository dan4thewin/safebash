---
title: "`$(<file)` forks instead of reading the file directly"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`s=$(<f)` emits an open/read/close, with no fork and no pipe"
  - "the value is byte-identical to today's (trailing newline stripped)"
  - "the emitted binary contains no reference to `/usr/bin/cat`"
  - "the slurped value is still TAINTED — file content is external input"
---

## What

`$(<file)` works and has the right semantics — it slurps the file and strips
the trailing newline (a 6-byte `a\nb\nc\n` yields `len` 5). But it is lowered
through the general command-substitution machinery: the emitted C shows
`sash_fork` alongside `sash_read_all`.

The entire reason the `$(<f)` spelling exists in bash is that it does **not**
fork — it is the cheap way to slurp a file. Here the syntax implies an
optimization the implementation does not perform.

## Why it matters more now

DD#99's `mapfile` replacement is built on it:

```sh
s=$(<f); attest s [[ -n $s ]]
matchall $s "[^\x0a]+" a || :          # mapfile -t
```

so `$(<f)` moves from an occasional convenience onto the common path for every
file-to-array read in a migrated program.

## Shape of the fix

The redirect target is a literal at compile time in the common case, so the
lowering can emit `open`/`read`-to-EOF/`close` directly into a value, with no
fork, no pipe and no child. `sash_read_all` already exists. A non-literal
target (`$(<$f)`) can keep the current path or take the same treatment with a
runtime path argument.

Correctness is not affected — this is purely the cost.

## Re-measured 2026-09-12 — it is worse than "forks"

It does not merely fork. `$(<file)` is **desugared at parse time into
`$(cat file)`** and the binary execs `/usr/bin/cat`:

`sash/src/parse/expr.rs:1465` — *"`$(<file)` — slurp a file via `cat`"*:

```rust
fn parse_slurp_form(scanner: &mut Scanner, span: Span) -> Result<Expr, Diagnostic> {
    // ...
    name: Expr { kind: ExprKind::Literal("cat".to_string()), ... }
```

Emitted C for `s = $(</tmp/f.txt)`:

```c
const char* __t0_argv[] = { "/usr/bin/cat", "/tmp/nr/f.txt", (const char*)0 };
i64 __t1_p = sash_capture_status((const char* const*)__t0_argv, sash_envp, &__t1_l, &__t1_st);
```

`sash_read_all` is never called. Verified on the linked binary: it is
`statically linked`, embeds the string `/usr/bin/cat`, and makes **two
`execve` calls** at runtime under `strace`.

## Why this is a bug, not an optimization

The original filing said "correctness is not affected — this is purely the
cost." That understates it:

1. **A static binary acquires a runtime dependency.** sash's entire value
   proposition is a self-contained static ELF. `$(<f)` silently makes the
   program require `/usr/bin/cat` on the *target* machine. Nothing in the
   source says so, and the failure appears only at run time on a host that
   lacks it.
2. **DD#23 resolves paths at compile time**, so this also makes *compilation*
   depend on `cat` existing on the build host.
3. **DD#99 put it on the common path.** Every migrated file-to-array read
   (`s=$(<f); matchall …` as the `mapfile -t` replacement) now spawns a
   process.

Reclassified P3/optimization → **P2/bug**.

## Taint gate — do not lose this in the fix

`$(<f)` is tainted today only as a *side effect* of being an external-command
capture. Moving to a direct `open`/`read`/`close` leaves the external-capture
path, and the taint must be re-established deliberately: **file contents are
external input and must stay tainted.** A fix that silently returns clean
data would punch a hole straight through the taint model. This is the single
highest-risk part of the change, hence the explicit gate above.

## Fix location

`sash_read_all` already exists and is already wired into codegen
(`codegen/dispatch.rs:1506`, `codegen/setup.rs:798`). The work is to stop
desugaring to `cat` in `parse_slurp_form` and emit a real slurp node that
lowers to open/read-to-EOF/close.

## Blocked 2026-09-12 — `sash_read_all` is not fit to build on yet

The "shape of the fix" above says to lower this through `sash_read_all`.
Reading that function first found that it **silently truncates at 64 KiB**
and treats a read error as EOF (`sash-rt/src/lib.rs:3621`). Building
`$(<file)` on it as it stands would trade one bug (forks `/usr/bin/cat`)
for a worse one (`s=$(<f)` silently yields a 64 KiB prefix of a larger
file) — and `cat` has no such limit, so it would be a straight regression
against today's behaviour.

Testing the existing `sash_read_all` caller — DD#99 `read <fd> ARRAY` —
against a large stream then turned up a **silent SIGSEGV** from heap
exhaustion at ~12 KB of payload. Both are filed together as
`bug-heap-exhaustion-silent-segv` (P1), which must land first.

The rest of this filing stands unchanged, including the taint gate.

## Unblocked 2026-09-13

`bug-heap-exhaustion-silent-segv` closed: option C landed 2026-09-12 and
option B as **DD#107** (256 MB heap) on 2026-09-13. `sash_read_all` now grows
a heap buffer with real error handling — the 64 KiB truncation that blocked
this task is gone.

## Done 2026-09-13

`$(<file)` is now a first-class node end to end: `ExprKind::SlurpFile` →
`SsaValue::SlurpFile` → one `sash_slurp_file` call in **both** backends. The
`cat` desugar in `parse_slurp_form` is gone.

All four gates verified on the live binary:

| gate | result |
|---|---|
| no fork, no pipe | `strace` shows **1** execve (the program), zero clone/pipe — was 2 |
| value byte-identical | `a\nb\nc\n`→5, `x\n\n\n`→1, no-trailing→10, empty→0 |
| no `/usr/bin/cat` in the binary | `strings \| grep -c` → 0, both backends |
| still tainted | spanned SB5020 at the use site |

### One runtime function, not two inline lowerings

`sash_slurp_file(path, len, &out_len)` opens `O_RDONLY`, `fstat`s to reject a
directory explicitly, then delegates the trailing-newline strip to
**`sash_capture_drain`** — literally the same code the general `$( )` capture
uses. The backends cannot drift, and if `bug-cmdsub-trailing-newlines` ever
changes the strip rule it changes both paths at once.

The `fstat` is not defensive padding: `open(O_RDONLY)` **succeeds** on a
directory and it is `read` that fails `EISDIR`, which fell through to the
path-less `sash_panic_read`. Failure paths also `set_last_errno` — `sash_emsg`
appends the errno reason, and without it the message reads `error 0`.

### Taint had to be asserted, not inherited

`SsaValue::SlurpFile`'s child is the **path**, so the any-tainted-composite
rule would have made `$(<f.txt)` — a literal path — read **clean**. It is
listed explicitly in the always-tainted arm of `ssa_taint.rs` alongside
`ArgvIndex` and `CmdSubstBody`. The exhaustive match there is what forced the
decision to be made rather than defaulted.

### Two more bugs the desugar was hiding

**Argument injection via filename.** A file named `-n`: `$(<-n)` became
`cat -n`, `cat` parsed it as a **flag**, read stdin instead, and the slurp
yielded nothing (`attest: s failed the gate`). bash yields the file contents.
Any filename reaching a real argv is an injection surface; a direct `open` has
no argv at all. Pinned as `sl.11`, the sharpest discriminator in the set.

**A taint hole introduced and caught mid-work.** `[[ $(<f) = x ]]` was
rejected before the rewrite and silently **accepted** after, because
`check_expr_taint` has a `_` fall-through. Found by diffing an 11-case
before/after behaviour matrix against stashed HEAD — not by any test that
existed. Fixed, and pinned as `sl.8`.

That is the shape of the whole change: adding `ExprKind::SlurpFile` broke 4
compile sites and `SsaValue::SlurpFile` broke 2, and every one of those was a
**decision surfaced**. The six integration points with `_` fall-throughs all
failed **silently** and each had to be found by testing.

### Specs

`sl.1`–`sl.11` in `spec/misc.sh`, unguarded in both modes. Fail-first on
revert fails exactly **2** per mode: `sl.2` (the diagnostic mentions neither
`cat` nor "command failed") and `sl.11` (the `-n` filename). The other nine
pass either way by design — they pin the *byte-identical* gate and the taint
behaviour, which the fix was required not to change.

### Follow-up policy (settled 2026-11-10)

This implementation preserved the earlier unchecked abort. DD130 now
classifies path-specific failures as recoverable, and DD88 accepts direct
file-read capture-conditions, with this attempt's bytes on handled failure.
Ordinary unhandled slurps still abort before assignment.
`dollar-lt-file-policy` is closed as a decision; the new handling remains
queued in `recoverable-file-failures`, not implemented by this older task.

DD22/DD99 already settle the filing's IFS question: no implicit splitting.
The historical change to a variable-naming taint diagnostic is not a
separate language policy. Neither point reopens this completed first-class
read implementation.
