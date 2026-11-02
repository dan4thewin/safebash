---
title: "A clean value split retains the target's previous taint"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "A clean value split over an argv-filled array permits a cell read without :! and prints clean"
  - "An empty value split clears the old cells and permits the empty-cell fallback"
  - "The same replacement works through a collection slot summary"
  - "Kernel-backed reads remain tainted and split input reads are still checked before replacement"
---

B106, corrected during W3 (journal 562). DD99 already settles the behavior:
**every read replaces its target, including empty input and EOF**. The old
finding's success-only premise was wrong. Runtime `sash_split_into_array`
clears the table before its empty-input return.

Both backends reject this with SB5020 after a dirty argument is supplied:

```
array a=()
argv a
read "clean" a
echo "${a[0]:-empty}"
```

Replacing `"clean"` with `""` is also rejected. With `--no-taint`, the two
programs print `clean` and `empty`, respectively; the runtime replacement
is correct.

The validator excludes value splitting from `tainted_read_target`, while
`forget_replaced_origins` removes only the target's ownership metadata.
Its old boolean taint remains, including in an independent slot summary.
The eventual repair must clear the complete replaced state, not relax
adjacency or change fd-read taint.

The recorded reference work did not settle this value-split case, and it
does not block the ordinary loop port; its command transfer is untouched
by W3. The old `w0/c5`
reproducer was a pipe read, not a value split, and is already correct under
W2a's read-boundary rule.

Evidence: `followups-*.tsv` and `backlog-*.tsv`, with the complete
`w3-backlog` traces, in session `files/port-w3-probes.tar.gz`.
