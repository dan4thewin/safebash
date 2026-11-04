---
title: "A clean value split retains the target's previous taint"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "Ordinary clean splitting preserves unresolved taint under DD128"
  - "Direct clean splitting on the target's failed-attest branch resolves it"
  - "A helper does not inherit its caller's replacement permission"
  - "Kernel-backed reads remain tainted and split input reads are checked"
---

Superseded by DD128, implemented in journal 568. The requested ordinary
clean-split declassification is now deliberately forbidden: replacing bytes
is distinct from resolving a live binding's taint. `adjresolve.14` permits
a direct clean split on its subject's failed-attest branch, while `.9`/`.16`
retain the helper/slot barrier. No runtime change is needed.

B106 was corrected during W3 (journal 562). DD99 settles the value behavior:
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

The old validator discarded ownership on replacement while retaining
boolean taint. Under DD128 both remain unresolved together outside legal
resolution; a direct failed-attest replacement clears both. The old proposal
to clear the complete state for every clean split is withdrawn, not deferred.

The recorded reference work did not settle this value-split case, and it
does not block the ordinary loop port; its command transfer is untouched
by W3. The old `w0/c5`
reproducer was a pipe read, not a value split, and is already correct under
W2a's read-boundary rule.

Evidence: `followups-*.tsv` and `backlog-*.tsv`, with the complete
`w3-backlog` traces, in session `files/port-w3-probes.tar.gz`.
