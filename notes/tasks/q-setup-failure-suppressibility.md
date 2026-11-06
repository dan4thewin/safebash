---
title: "Question: may `-` discard a setup failure (fork, pipe creation, a redirect's open)?"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG rules (a), (b), (c) or another option; DD#55 and DD#70 record the ruling"
  - "every row of the evidence tables prints what the ruling says, in both backends, with specs"
  - "`bug-fork-failure-reporting-by-form` and `retire-suppression-on-compounds` are updated to the ruling"
  - "DD128's fatal arithmetic/heap behavior and terminal exit/fail semantics remain unchanged"
---

DG, 2026-10-28: "the point of suppression is to say I-don't-care about an
external program or maybe a system call result, to make a better
alternative to the || : bash spelling."

DG, 2026-10-30: "I'm not at all sure that failure to fork, create a pipe,
or allocate memory should be suppressible..."

## Current boundary (2026-11-06)

DD128 now settles heap exhaustion and arithmetic faults: they are fatal,
not suppressible results, and handler dispatch never resumes the
interrupted code. Do not reopen that choice. The remaining question here
is whether `-` discards fork/pipe/redirect setup failures as well as the
command's own result. Existing condition handling is a separate capability;
choosing (a) does not remove a condition's ability to catch redirect open.

## Evidence

The tables are in `q-suppression-on-user-functions.md`, section Q5. They
cover a redirect's open (y, z, k and e rows) and fork failure injected at
the first `clone` (y13–y17, k01–k04, u04–u06, j1–j4). The "F2 trial"
column records the post-F2 behavior at filing; the fresh subset below
does not claim to remeasure the whole table. In short:

- Statement-level `-` never discards a redirect's open failure (y02, y08,
  y11, k05; DD#70 :159–161). A condition does (e1–e5).
- A stage `-` discards it, because the open runs in the child
  (y10, z01, z03, z05).
- A job `-` discards a fork failure (y14, y16, j1, j4).
- A pipeline's fork failure aborts even with a `-` on every stage
  (k01, k02). Only a `-` group discards it (p518.22/.23).

At filing, allocation failure under `-` and pipe-creation failure under `-`
were unprobed. Heap exhaustion's policy is now settled by DD128; the
`pipe2` row still needs a direct control.

### Current native subset, 2026-11-06 at `a7787b24`

Each program ends with `echo continued`, and `missing/` is absent:

| form and injected fault | result |
|---|---|
| `- /bin/true &`, first `clone` fails with EAGAIN | prints `continued`, exit 0, empty stderr |
| `- /bin/true > missing/out` | no `continued`, exit 1, missing-path diagnostic |
| `- /bin/true > missing/out \| /bin/cat` | prints `continued`, exit 0, empty stderr |

The uninjected job control also prints `continued` and exits 0; its trace
contains a successful `clone`. The fault trace records the first `clone`
as EAGAIN with `INJECTED`. These are native-only probes; C was not re-run.
Evidence: session `files/setup-policy-20261106/`. No spec-history rows
were written.

## Options (from Q5)

- (a) `-` never discards a setup failure, in any position. On a stage or a
  job it discards only the process's own status.
- (b) `-` discards a setup failure wherever `||` or a condition handles
  one. This reverses DD#70 :159–161.
- (c) Keep the split, and document which positions discard one.

DG's 2026-10-30 remark leans toward (a). Under (a), p518.22/.23 flip,
`retire-suppression-on-compounds` needs no new spelling, and a job's `-`
(`- f &`, which F6 keeps legal) discards only the child's report of its
own status (x16).
