---
title: "`$(<file)` forks instead of reading the file directly"
status: ready
priority: P3
kind: optimization
depends: []
gates:
  - "`s=$(<f)` emits an open/read/close, with no fork and no pipe"
  - "the value is byte-identical to today's (trailing newline stripped)"
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
