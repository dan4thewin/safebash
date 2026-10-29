# 542 - A capture body is entered like a function body

2026-10-29. DG said "go for F5", Order item 12 of the batch-531 port:
capture bodies are function bodies. F5 holds eleven findings in four
groups, so it lands as four commits (decide-and-report):
- A, this one: what a body entry resets (B57, B59, B60, B83, B83b, and
  B101, folded in);
- B: int-function arity (B56) and the signature scan (B53, with B49);
- C: B73;
- D: the capture fast paths (B71, B74).
B58, the named-function twin of B59, landed with F13 (journal 532).

## The bugs

DD86 (2026-09-29) says a `$( … )` body is a function body. DD27 forbids
recursion except a self tail call. So entering any body must drop three
things the caller had: its place on the recursion stack, its loop context
and its condition context. The capture routes dropped none of them, and the
plain inliners kept the condition context:
- **B57** (DD27). `capture_func_into` never looked at `state.inlining`, so
  `$(f)` inside `f` recursed in the compiler until its stack overflowed,
  rc 134: assigned, as an argument, in a capture condition, and as
  `$(- f 1)` (cap/c1, c2, c12, c16; ua/a1, a4). A cycle through a capture
  was caught only at a later plain call (c10).
- **B59** (DD27). `capture_body_into` kept the enclosing function's tail
  loop. In `fn g { … str s=$(echo x; return g $(( $1 - 1 ))) … }` the
  `return g` branched to `g`'s loop header from inside the open capture:
  no output, status 0 (cap/b5, b8; ua/a2).
- **B60** (DD86). validate's loop-context walk carried `in_loop` into
  `$( … )` bodies, so `for x in a b { str s=$(echo $x; break) … }`
  compiled. The `break` left the open capture and every later write was
  lost, status 0 (cap/b1, b3, b7).
- **B83** (DD55, DD86). The inliners lowered a body with `in_condition`
  still raised, so a function called as a condition ran its whole body
  unchecked. `fn h { : && /bin/false; echo after; return 0 }; if h { echo
  yes }` printed `after` and `yes` (cx/c2, c7, c8, i2, i4; cx2/k4 for an
  `-> int` call in `(( ))`). That is the bash `set -e` blind spot which
  DD55's comparison says sash does not have.
- **B83b** (DD55). The same leak into an anonymous body in a condition
  command's arguments: `if /bin/echo "[$(/bin/false | /bin/cat)]" …` printed
  `yes`. An external capture is tainted, so it shows only under
  `--no-taint` (B83b-1; cx2/k1, k2, k6).
- **B101** (DD68). `lower_while` pushed the loop context after lowering the
  condition, so `while break` inside a `for` left the `for` (b101/w1, w4
  for `until`). The ledger filed its two hunks under W3, but both are the
  transform's, and the findings index already said "lands with F5's B60".
  Folded in, decide-and-report. W3 still re-probes the walker's view.

## The port

The branch's code ports as written:
- `functions::reject_recursive_call` reports SB5037 at the statement when
  the callee is being inlined. The plain and int routes already checked;
  now all three share it, and the capture route has it at last. The int
  route keeps its exemption for a suppressed tail entry.
- `functions::lower_inlined_body` lowers a body for all three routes. It
  sets the return target, takes the tail loop and the condition context,
  lowers, and restores all three.
- `capture_body_into` takes the tail loop and `in_condition` for an
  anonymous body.
- `descend_loop_control` walks structural bodies with the loop context and
  `$( )` bodies with none; `check_loop_control_context`'s doc says why.
- `lower_while` pushes the loop context before the condition.

Suppression is not touched. Per DG's 2026-10-28 ruling, nothing here may
cement `-` on user functions; `lower_inlined_body`'s doc points at
`q-suppression-on-user-functions`.

`lower_while` still lowers a discarded first pass before the push, so a
`break` in that pass targets the enclosing loop. Its blocks are thrown
away, so nothing observes it; `loop_target`'s doc now says where the push
happens.

## Probes

Pre-port against the ported binary, native, 101 pairs (every cap/, cx/,
ua/ and b101/ probe; cx2/* and B83b-1 also under `--no-taint`): 27 changed,
74 unchanged, every change in the predicted direction.

| Probes | Before | After |
|---|---|---|
| cap/c1, c2, c12; ua/a1 | compiler stack overflow, rc 134 | SB5037 |
| cap/c16 | stack overflow | SB5037 + SB5043 |
| ua/a4 | stack overflow | SB4025 + SB5037 |
| cap/c10 | SB5037 at a later call | SB5037 at the capture |
| cap/b5, b8; ua/a2 | empty output, rc 0 | SB5037 |
| cap/b1, b3, b7 | empty output, rc 0 | SB5081 |
| cx/c2, c7, c8, i2, i4 | failure swallowed | rc 1, "/bin/false: command failed (exit 1)" |
| cx/c9 | failure swallowed | the stage child aborts; the condition consumes it |
| cx2/k4; k1, k2, k6, B83b-1 (`--no-taint`) | `yes` | rc 1 abort |
| b101/w1, w4 | `done` | `after while 1`, `after while 2`, `done` |

Three rows look odd and are right:
- **c16.** The SB5043 is a genuine DD81 arity error: the call passes an
  argument that `f` never reads. The crash used to hide it.
- **c10.** In `g → $(f) → g` the cycle is now reported at the capture, the
  first edge that closes it, at the statement (F13's `error_at_stmt`).
- **c9.** `h` runs as the final stage's child, so its body's abort ends
  that child, with its own report, and `if` consumes the child's failure.
  Which stage of `/bin/echo x | /bin/false` the report names depends on the
  SIGPIPE race (DD55's pipeline rule); the verdict does not. The specs use
  `/bin/false | /bin/cat` to stay out of the race.

Unchanged controls include every positive capture, loop and condition
form, a top-level and an in-function `while break`, and the default-flag
runs of k1, k2, k6 and B83b-1, which stay SB5020.

## Specs

- functions.sh rec.5-rec.9 (plan 230): SB5037 for `$(f)` assigned and as an
  argument, the anonymous `return g` assigned and as an argument, and
  SB4025 + SB5037 for a capture condition.
- control_flow.sh cf.n19-cf.n23 (plan 114): SB5081 for `break` and
  `continue` in a capture in a loop; cf.n21, a loop wholly inside a
  capture, is the control; `while break` and `until break` in a `for`.
- errors.sh 55.76-55.87 (plan 676): the branch's twelve, ids kept. All
  twelve programs match the branch's byte for byte (comment lines aside);
  the header now cites this journal.

Negative control on the pre-port binary, both modes: 21 of 22 red; cf.n21
passes. Reversion stubs, each red in both modes, pin all 21:

| Stub | Change | Red |
|---|---|---|
| a | no `in_condition` take in `lower_inlined_body` | 55.76-55.83, 55.87 |
| b | no take in `capture_body_into` | 55.84-55.86 |
| c | `$( )` bodies keep `in_loop` | cf.n19, cf.n20 |
| d | no reject in `capture_func_into` | rec.5, rec.6, rec.9 |
| e | tail loop cloned, not taken | rec.7, rec.8 |
| f | push after the condition | cf.n22, cf.n23 |

## The gate

`NEW=20 mk gate`: GATE PASS. Rows 6471863-6482107, 10245 = 10232 pass +
13 skip (5233 native, 5012 C), 0 flips, 0 retired, 0 ghosts, liveness 260.
First-ever is 20, not 44: 55.76-55.87 have rows from the branch's sweep of
the same programs, so they are neither new nor flips.

## Ledger

Rows ported: H174, H176, H177, H189, H190, H205, H207, H236, H238, H241,
H312, H313. H160 (an ssa.rs doc, SB5033 → SB5037) landed with F13 but was
never marked; it is marked now. H206 and H325 stay pending for their other
halves (B56 in unit B; 55.88 in F6). Findings B57, B59, B60, B83, B83b and
B101 are ticked.

## Backlog

Nothing new. The F5 questions the plan lists (capture-hoisting order,
`exec` in a capture (B34), captures writing outer names) are filed with a
later unit.

## Narrative

Every bug here is one omission seen from a different route: a body entry
that forgot to drop something the caller held. The branch's fix makes the
three inline routes share one entry, so the next thing a body must drop
is dropped in one place. B101 is the same shape from the other side: a
loop that took up its context too late.
