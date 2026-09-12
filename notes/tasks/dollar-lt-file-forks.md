---
title: "`$(<file)` forks instead of reading the file directly"
status: ready
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
