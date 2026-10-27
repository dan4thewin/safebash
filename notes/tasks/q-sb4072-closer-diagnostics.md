---
title: "Question: which code and position for a missing command before a closer, and should SB4072 name the open block?"
status: open
priority: P4
kind: question
depends: []
gates:
  - "DG rules on Q1 and Q2; the rulings are recorded in DD#68 beside its F1+F4 addendum"
  - "o1-o5 give the ruled code at the ruled position, in both backends, and a spec pins each shape"
---

## What

Found 2026-10-27, while porting F1+F4 (journal 537). F1+F4 made a stray
block keyword SB4072 wherever a command may stand (DD#68 addendum
2026-10-27). That leaves two ways of reporting an operator with no command
after it:

| Id | Program | On 8e28c61 | With F1+F4 | bash -n |
|---|---|---|---|---|
| o1 | `/bin/echo a &&`, then end of input | SB4056 "`&&` has no right-hand side" at 1:13, the `&&` | the same | "unexpected end of file" |
| o2 | `/bin/echo a && fi` | SB5011 "command 'fi' not found" at 1:16 | SB4072 at 1:16, the `fi` | "near unexpected token `fi'" |
| o3 | `if /bin/true {` / `  /bin/echo a &&` / `}` | SB4072 at 3:1, the `}` | the same code and position, new text | "near unexpected token `}'" |
| o4 | o3 with `\|` for `&&` | SB4072 at 3:1 | the same, new text | "near unexpected token `}'" |
| o5 | `/bin/echo a \| done` | SB5011 at 1:15 | SB4072 at 1:15, the `done` | "near unexpected token `done'" |

The closer itself is misplaced only because the operator before it still
needs a command. The `}` text says so ("never right after a `|`, `&&`, `||`
or `!` that still needs a command"); the keyword texts do not. bash reports
every row but o1 at the closer.

### Q1. Code and position

- (a) Keep them: SB4072 at the closer, SB4056 at the operator only at the
  end of the input. This matches bash.
- (b) SB4056 at the operator wherever its right-hand side is missing,
  whether the input ends or a closer follows. One code and one position for
  one mistake, and SB4056's text already names it. o2-o5 change code; o3
  and o4 change position as well.

### Q2. Naming the open block

SB4072's text gives the rule, not the case: "a `fi` ends the innermost open
block only if that block is an `if … then` body …". It does not say which
block is open. Naming it ("the innermost open block is the `while` at 3:1")
needs the parser to keep a stack of open blocks, which it does not have:
each block is a recursive call.

No recommendation yet. Neither is a bug: every row is rejected, at a
position that points at the mistake. Filed under DG's rule for the port that
questions and uncovered cases become backlog tasks (2026-10-24).

Probes: `files/port-f1f4-probes.tar.gz`, directory `qq/` (o1-o5).
