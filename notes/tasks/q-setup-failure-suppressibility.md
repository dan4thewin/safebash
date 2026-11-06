---
title: "Resolved: failure recovery depends on cause and state, not setup position"
status: done
priority: P2
kind: question
depends: []
gates:
  - "DD130 records DG's recoverable/fatal distinction, fork fatality and recoverable path-specific file errors; DD55/DD70/DD128 agree"
  - "implementation and native/C proof obligations from the evidence tables have explicit open backlog owners"
  - "`bug-fork-failure-reporting-by-form` and `retire-suppression-on-compounds` are updated to the ruling"
  - "DD128's fatal arithmetic/heap behavior and terminal exit/fail semantics remain unchanged"
---

DG, 2026-10-28: "the point of suppression is to say I-don't-care about an
external program or maybe a system call result, to make a better
alternative to the || : bash spelling."

DG, 2026-10-30: "I'm not at all sure that failure to fork, create a pipe,
or allocate memory should be suppressible..."

## Resolution (2026-11-06)

DG chose a different axis from the three original options: **recoverable
versus fatal**, not setup versus execution. Catching or suppressing a
recoverable failure permits predictable, useful continuation. A fatal
fault must stop the executing process because continuing is worse.
DD130 is the authority; journal 581 records the decision.

Fork failure is fatal like heap exhaustion: neither a condition, `||`,
`!` nor `-` may resume after it. Path-specific file errors are recoverable,
including an attached redirect's failed open. Its command must not run or
write to the original descriptor. Unhandled recoverable failures still
abort normally. Resource/error-state classification is implementation
work, not permission to label every setup failure alike.

The question is closed, **not the implementation**. DD128's existing
arithmetic/heap, handler ownership and explicit exit/fail rules remain.
The historical observations below are retained as evidence, not desired
behavior or a claim that every row has been remeasured.

## Evidence

The tables are in `q-suppression-on-user-functions.md`, section Q5. They
cover a redirect's open (y, z, k and e rows) and fork failure injected at
the first `clone` (y13–y17, k01–k04, u04–u06, j1–j4). The "F2 trial"
column records the post-F2 behavior at filing; the fresh subset below
does not claim to remeasure the whole table. In short:

- Statement-level `-` never discards a redirect's open failure (y02, y08,
  y11, k05; the former DD70 redirect rule). A condition does (e1–e5).
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

## Implementation owners

- `runtime-failure-classification`: shared operation/cause/state policy,
  including resource errors and the actual ERR effect model.
- `bug-fork-failure-reporting-by-form`: fatal fork handling across all
  forms, replacing the old caught-fork and negation behavior.
- `recoverable-file-failures`: real catch/suppress handling for ordinary
  file errors, including the statement/stage redirect mismatch.

`retire-suppression-on-compounds` retains this completed decision as its
dependency. Fatal construction faults need no replacement discard spelling.
No new process-tree failure protocol or function-result coercion is implied.
