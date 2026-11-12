title: "Question: is a pipe stage known never to write (on the left) or never to read (on the right) an error?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules (a) or (b); DD#55 or DD#122 records the ruling"
  - "under (a), n1, n2 and n4's g are rejected with a diagnostic naming the rule, in both backends, and n6 stays legal; the flipped specs are rewritten"

Moved here from `q-suppression-on-user-functions` (Q6) when that task
closed at the F6 wrap-up (journal 549). That section keeps the full
evidence table.

DG, 2026-10-28, on B81: a stage the compiler knows never writes to its
stdout should be an error on a pipe's left, and one it knows never reads
its stdin should be an error on the right.

| Id | Program | Result |
|---|---|---|
| n1 | `fn f { return 3 }` then `f \| /bin/cat` then `echo after` | rc 3, "f: command failed (exit 3)" |
| n2 | `: \| /bin/cat` then `echo after` | `after` |
| n6 | `fn f { /bin/true }` then `f \| /bin/cat` then `echo after` | `after` |
| n4 | `fn g { echo y }` then `/usr/bin/yes \| g` then `echo after` | `y`, rc 141, "/usr/bin/yes: command failed (exit 141)" |
| n5 | n4 with `- /usr/bin/yes \| g` | `y`, `after` |

- (a) DG's lean, limited to what the compiler knows in-process. A stage
  known never to write is an error on a pipe's left (n1, n2). A stage
  known never to read is an error on its right (n4's g). n6 stays legal,
  because `/bin/true` is an external. The left half needs W2's T1
  reaches-stdout analysis; the right half needs a new reads-stdin
  analysis. Flips pf.51, p518.10, p518.28, p518.29, pipeown.32 and 24.29,
  each of which pipes `:`.
- (b) Keep these pipes legal.

It cannot land before W2, the batch-531 port's stdout-taint step (not a
task file of its own). DG's taint remark, that `cmd "$(g)" | cat` should be a taint
error unless g is known to be clean, is W2's T1 (`batch-531-findings`
§18), and is already planned there.
