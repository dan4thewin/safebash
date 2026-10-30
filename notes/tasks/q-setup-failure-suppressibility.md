---
title: "Question: may `-` discard a setup failure (fork, pipe creation, allocation, a redirect's open)?"
status: open
priority: P2
kind: question
depends: []
gates:
  - "DG rules (a), (b), (c) or another option; DD#55 and DD#70 record the ruling"
  - "every row of the evidence tables prints what the ruling says, in both backends, with specs"
  - "`bug-fork-failure-reporting-by-form` and `retire-suppression-on-compounds` are updated to the ruling"
---

DG, 2026-10-28: "the point of suppression is to say I-don't-care about an
external program or maybe a system call result, to make a better
alternative to the || : bash spelling."

DG, 2026-10-30: "I'm not at all sure that failure to fork, create a pipe,
or allocate memory should be suppressible..."

## Evidence

The tables are in `q-suppression-on-user-functions.md`, section Q5. They
cover a redirect's open (y, z, k and e rows) and fork failure injected at
the first `clone` (y13–y17, k01–k04, u04–u06, j1–j4). F2 has landed since,
so the "F2 trial" column is today's behaviour. In short:

- Statement-level `-` never discards a redirect's open failure (y02, y08,
  y11, k05; DD#70 :159–161). A condition does (e1–e5).
- A stage `-` discards it, because the open runs in the child
  (y10, z01, z03, z05).
- A job `-` discards a fork failure (y14, y16, j1, j4).
- A pipeline's fork failure aborts even with a `-` on every stage
  (k01, k02). Only a `-` group discards it (p518.22/.23).

Not yet probed: allocation failure under `-` (with `--heap-size`), and
pipe-creation failure (`pipe2`) under `-`.

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
