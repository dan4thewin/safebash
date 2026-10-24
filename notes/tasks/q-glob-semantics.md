---
title: "Questions: glob dotfiles, order, open errors, non-final `*`"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG rules on each of the four questions below; the rulings are recorded as a DD#39/DD#69/DD#73 addendum"
  - "each ruling is pinned by a spec in both backends"
---

## What

DD#39 (star-only globs), DD#69 (globs as arrays) and DD#73 (absolute paths)
say nothing on four points. The runtime (`sash_glob`, `sash-rt/src/lib.rs`)
answers each one implicitly. Measured during the batch-531 port (F13) in
both backends, in a directory holding `.hid`, `a*c`, `ab.c`, `abc`,
`axbxc`, `sub1/f1` and `sub2/f2`:

1. **Dotfiles.** `for f in * { … }` yields `.hid`. Bash yields a
   leading-dot name only for a pattern segment that itself starts with `.`.
2. **Order.** Results come in `getdents64` order: `sub1 abc axbxc a*c sub2
   .hid ab.c` here (ext4). Spec g.6 asserts membership only, so no spec
   pins an order.
3. **Directory-open errors.** A directory that cannot be opened yields no
   matches, silently, whatever the errno. DD#73 rule 3 makes a failed
   `getcwd` abort through DD#55 ("must not silently fall back").
4. **A `*` in a non-final segment.** `*/f1` and `sub*/f*` compile and yield
   nothing.

## Recommendations (made during F13, not yet ruled)

1. Bash's rule: a leading `.` must be matched explicitly.
2. A bytewise sort, so a script's behaviour does not depend on the
   filesystem.
3. ENOENT and ENOTDIR mean no matches; any other errno (EACCES, EMFILE,
   EIO) aborts through DD#55, in the spirit of DD#73 rule 3.
4. Support it with a per-segment walk. The alternative is a compile-time
   rejection under a new code. Silence is not an option either way.

Item 4 overlaps `bug-glob-directory-portion` (a literal directory portion),
and both touch the same runtime walk. Item 1 decides whether `.hid` appears
in a spec's expected output.
