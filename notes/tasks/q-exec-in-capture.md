---
title: "Question: what should `exec` inside a `$( )` body do?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules (a) reject, (b) fork the body, or (c) keep and document; the ruling is recorded in DD#86"
  - "specs in both modes pin the ruling, for a direct `exec` and for one reached through a called function"
---

This is batch-531's B34 (`batch-531-findings.md`, section 12). A `$( )` body
is a function body, and sash has no subshell (DD#86 addenda 2026-09-28 and
2026-09-29), so an `exec` inside one replaces the whole program. The capture
never completes, and nothing after it runs.

Native and C are identical:

```
echo before
str x=$(exec /bin/echo hi)
echo "after [$x]"
```

The program prints `before`, then exits 0 without printing `after`. `hi`
goes into the capture, which never completes, so nothing reads it. Calling
a function from the capture gives the same result
(`fn f { exec /bin/echo hi }; str x=$(f)`). Bash forks the capture, so it
prints `after [hi]`. cap.35 and cap.36 pin today's behaviour; a ruling
other than (c) rewrites them.

Until journal 546 the output also depended on which capture path the site
took. The purity walk (`capture_purity.rs`) took the bare builtin `exec` for
an in-process write, so a body with nothing else in it took the userspace
buffer path, and the exec'd command wrote `hi` to the program's real stdout
(`before`, `hi`, measured at ab133dd2). A body made impure by an unrelated
external (`fn f { /bin/true; exec /bin/echo hi }`) already printed `before`
only. Journal 546 marks `exec` impure, so both paths agree. That is a fix to
an optimisation, which must be unobservable, and it does not answer this
question.

Hand-parser compiles the program with taint on: the capture's head is the
builtin `exec`, and the capture never returns a value. The branch reports
SB5020 instead, because its walker treats any body that runs a command as
tainting the capture (B114). Neither is a rule about `exec`, and no DD
mentions `exec` in a capture.

The fd-only form is not affected. `exec >/dev/null` in a capture only
rebinds this process's descriptors. It parses as a redirected statement,
which the purity walk already treats as impure, and it prints `[]` then
`after` in both backends.

## Options

- **(a) Reject** `exec` in a `$( )` body, the way SB4022 rejects `a | exec b`
  as a stage. A walker over capture bodies catches the direct case.
  Catching an `exec` reached through a call needs a transitive "this body
  runs exec" fact, which W1's per-function summaries will provide.
- **(b) Fork the body** when it runs `exec`, directly or through a call.
  The `exec` then replaces a child, and the capture receives its output,
  which is bash's answer. The cost is that the body's global writes would be
  lost, which gives a second capture semantics against DD#86's "no
  subshells". DG's 2026-09-28 wording ("unless a fork is required") leaves
  room for it.
- **(c) Keep it, and document it.** `exec` ends the program wherever it
  stands, and DD#86 says where its output goes.

The AI recommends (a). A capture that cannot return is never what its author
meant, and it matches how SB4022 treats `exec` as a stage. The decision is
DG's.

Probes: the session archive `files/port-f5-probes.tar.gz` (`q/b34a.sh`-
`b34c.sh`; `px/p1.sh`-`p4.sh` for the two capture paths, `px/f1.sh`-`f4.sh`
for the fd-only form).
