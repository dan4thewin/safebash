---
title: "Bug: a glob with two or more `*` matches only names containing a literal `*`"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`a*b*c` yields `ab.c`, `abc` and `axbxc` (bash's rule; a name literally `a*c` does not match), in both backends"
  - "a spec pins a multi-star pattern in both backends; it fails on the pre-fix binary"
  - "the `sash_glob` doc comment lists what the matcher supports, accurately"
---

## What

In a directory holding `abc`, `axbxc`, `ab.c` and `a*c`:

```sh
for f in a*b*c { echo "$f" }
```

This prints nothing, in both backends; it compiles cleanly and exits 0.
Bash prints `ab.c`, `abc` and `axbxc`.

## Cause

`sash_glob` (`sash-rt/src/lib.rs`) splits the pattern at the **first** `*`
and matches `starts_with(prefix) && ends_with(suffix)`. For `a*b*c` the
suffix is `b*c`, compared byte for byte, so only a name literally ending in
`b*c` (a file named `axb*c`) matches. Its doc comment lists `"*.ext"`,
`"prefix*"`, `"prefix*suffix"` and `"*"`, the one-star shapes. Nothing at
compile time rejects a second star.

DD#39 reads "Star-only glob for files (`*` only, no `?` or `[abc]`)". It
does not limit the number of stars, so a multi-star pattern is in the
language and the silent empty result is a bug.

## Fix

Either a general star matcher (segments between stars, matched greedily
left to right, first anchored at the start and last at the end), or a
compile-time rejection with a DD#39 addendum. The first follows DD#39 as
written. It is independent of `bug-glob-directory-portion` but touches the
same function, so the two may land together.

Found during the batch-531 port (F13 glob probes, `m1`), unrelated to the
port itself.
