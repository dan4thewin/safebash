# 546 - `exec` in a capture skipped the buffer

2026-10-29. F5 unit E and the F5 wrap-up (Order item 12; journal 542 has
the plan). Unit E is a bug found while writing B34's question task. It sits
in `command_is_pure`, which unit D rewrote, so the 2026-10-26 rule fixes it
now. Decide-and-report: it is a bug by an established pattern (below), and
the fix is one condition.

## The bug

`capture_purity` chooses how a capture site holds its stdout: a userspace
buffer, which sees only `sash_write`, or a memfd, which sees everything
written to fd 1 (journal 545 has the two paths). `command_is_pure` treated
every bare command name other than a user function as a builtin or
intrinsic that writes through `sash_write`. `exec` is a bare builtin, but
`exec CMD` replaces this process with CMD, and CMD writes fd 1 itself. On
the buffer path, fd 1 is the program's real stdout.

The probes print `before`, then capture, then print `after [$x]`. Native
and C agreed, before and after:

| Probe | Capture | Path before | Before | Now |
|---|---|---|---|---|
| p1 | `str x=$(exec /bin/echo hi)` | buffer | `before`, `hi` | `before` |
| p3 | `fn f { exec /bin/echo hi }`, `$(f)` | buffer | `before`, `hi` | `before` |
| p4 | `fn f { /bin/true; exec /bin/echo hi }`, `$(f)` | memfd | `before` | `before` |

All three exit 0, and none prints `after`: the exec'd command is the
program's last act. In p4 an unrelated external made the body impure, so
`hi` went into the memfd, which nobody reads. That is the reference
behaviour. The buffer is an optimisation and must be unobservable: cap.20
pins that rule for a bare-called function wrapping an external, and B71
(journal 545) fixed the same leak for a value call by marking the site
impure. The fix does the same: `command_is_pure` answers false for
`BuiltinCmd::Exec`, beside the slash test.

The fd-only form is unaffected. `exec >/dev/null` parses as a redirected
statement, which the walk already treats as impure; probes f1-f4 print `[]`
then `after` on both paths, before and after.

## What this does not settle

What `exec` in a capture should do at all is B34, now
`notes/tasks/q-exec-in-capture.md` (P3): (a) reject, (b) fork the body, or
(c) keep and document. The AI recommends (a); DG decides. Under (a) this
fix becomes unreachable. Under (b) and (c) it is what keeps the two paths
consistent. cap.35 and cap.36 pin today's behaviour, and a ruling other than
(c) rewrites them.

B34's repro compiles with taint on: the capture's head is the builtin
`exec`, which never returns a value. The branch's SB5020 is B114's verdict,
so the `${x:!}` in the old repro was never needed on hand-parser.

## Specs and gates

cap.35 (direct) and cap.36 (through `fn f`), dual mode; misc.sh plan
123 -> 125. Negative control: on ab133dd2's binary both are red in both
modes and nothing else is; after the fix both are green. For a one-site fix
the pre-fix binary is the reversion proof.

`NEW=4 mk gate`: GATE PASS, rows 6512975-6523285, 10311 = 10298 pass + 13
skip, 0 flips, 0 retired. The four first-ever ids are cap.35 and cap.36 in
both modes, and nothing else.

## F5 wrap-up

- **Outer-name writes** are settled. DD86's 2026-09-28 addendum records
  DG's "changes to globals … survive unless a fork is required", so F5's
  simplification candidate "captures may not write outer names" is closed
  with no task (decide-and-report).
- **Capture-hoisting order** is `notes/tasks/q-capture-hoisting-order.md`
  (P2): (a) keep hoisting, or (b) evaluate left to right. Its two `$?` rows
  are DD55 bugs under either option. ev6 prints `[0] ` where DD55 says
  `[3]`, and ev5 compiles where DD55 says SB5012. Their fix waits for the
  option, because it decides how the fix is built: under (b) ev6 needs no
  special case. Re-probed at ab133dd2 (hn1 `new X new`, ev5 ` [0]`, ev6
  `[0] `), unchanged.
- The task file corrects an error carried from Step 0. On hand-parser,
  dispatch patterns and `[[ && || ]]` operands are evaluated eagerly too;
  the lazy forms are the branch's F8 (B87) and F9 (B86), not yet ported. The
  findings index said otherwise and now agrees.
- Probes (q/, px/ and the unit A-D directories) are archived, text only,
  in the session's `files/port-f5-probes.tar.gz`.

F5 is done in five units (journals 542-546): A (recursion, loop control
and condition context in bodies; B101 folded in), B (arity and the
signature scan), C (the shared arithmetic walker), D (value calls in
captures) and E. That is 12 of the 20 Order items. Next is F6 (Order item
13), which opens with B15 and waits for DG's go.
