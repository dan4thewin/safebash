title: "Bug: SB5012 says \"no preceding command\" when a command did precede and its status was discarded, scoped away or consumed"
status: open
priority: P4
kind: bug
depends: []
gates:
  - "each row of the table gets a message naming its actual reason; only the two rows with nothing before the read keep \"no preceding command\""
  - "the code stays SB5012 and every verdict is unchanged, in both backends"
  - "a spec pins each reason's text in both backends, and 55.15 asserts the code and text instead of bare `-n $err`"

## Found

2026-10-27, while porting F1+F4 (journal 537): a probe of `- f` for a
status-returning `f` drew it. Pre-existing at `8e28c61`, unchanged by F1+F4,
and the same on `taint-walker-redesign` (`e98fca8`); both backends.

Every row gets the same text: "$(status) is not available here — no
preceding command (the punctuation sugar $? reads the same value)".

| Program | Position | Actual reason (verdict is right) |
|---|---|---|
| `fn f { return 3 }` / `- f` / `echo "[$?]"` | 3:6 | `-` discarded it (DD55:25-28, and the table at 38) |
| `- /bin/false` / `echo "[$?]"` | 2:6 | the same |
| `/bin/false \|\| /bin/true` / `echo "[$?]"` | 2:6 | scoped to the construct's handling expression (DD55:41-43) |
| `if /bin/true { : }` / `echo "[$?]"` | 2:6 | the same |
| `if /bin/false { : } else { /bin/true; echo "[$?]" }` | 1:44 | consumed: only the first expression of a handling branch may read it (DD55:43-47) |
| `if /bin/true { /bin/true; echo "[$?]" }` | 1:32 | the same |
| `/bin/false \|\| { /bin/true; echo "[$?]" }` | 1:33 | the same |
| `echo "[$?]"` as the first statement | 1:6 | no preceding command: the message is right |
| `fn f { echo "[$?]" }` / `f` | 1:13 | a function body starts with no status: right |

Control: `/bin/true; echo "[$?]"; echo "[$?]"` prints `[0]` twice.

Probes: `files/port-f1f4-probes.tar.gz`, directories `dr/`, `fin2/` (dr8,
dr9) and `t9/` (b1, b3, b4, c1-c5).

## Why it is a bug

The message states a cause that is false in seven of the nine rows, and it is
the user's only guide to the rule. It has misled this project twice: journal
495 wrote a spec on its premise ("I reached for `- cmd` then `$(status)` and
got SB5012: no preceding command. There plainly was one."), and the `rdr.42`
comment records a redirect case where it named "the one thing that had just
happened".

## Cause

`emit_status_unavailable` (validate.rs 3591-3603) has one text, and the flow
state cannot tell it more: `StatusState` is `Undefined | Available`
(3249-3252), so the reason is gone by the time a read is checked. `Undefined`
is produced at:

- the program start (53);
- a function body's entry (`check_dq_function`, 3381);
- after a construct (`FlowState::end_construct`, 3298);
- after a status read (`check_dq_leaf`, 3457);
- after `-` (`leaf_status_after`, 3553);
- after a handling branch's first statement (`check_dq_body_branch`, 3750).

## Fix direction

Carry the reason in the state (`Undefined(Why)`, set at each producer above)
and render it in `emit_status_unavailable`. A message for the consumed case
should say that a command has run since the branch was entered, and name the
first-expression rule.

`bug-handling-branch-drops-latch` changes `check_dq_body_branch` too. Either
can land first.

## Not this task

`55.15` asserts only `-n $err`, so it would pass with SB5012 deleted. The fix
strengthens it, but the verdicts here are right.
