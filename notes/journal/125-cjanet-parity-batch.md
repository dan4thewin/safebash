# 125 — c/janet backend-parity batch

Session 2026-07-03. DG: "tackle the whole batch of work" (the c/janet
backend-parity cluster — the biggest ready batch). Six tasks, spec-first,
one commit per logical change, full-suite regression diff each time.

## Result

Native gate 0-failing throughout. **Zero regressions across the entire
batch.** Pre-existing failing baselines:

| backend | before | after | delta |
|---------|-------:|------:|------:|
| c       |     58 |    52 |    −6 |
| janet   |    106 |    75 |   −31 |

## What shipped

1. **janet pipeline EAGAIN flakiness** (567b8fe). Root cause: janet's
   `:out :pipe` / `os/pipe` default ends are non-blocking (ev loop), so a
   child inheriting one read EAGAIN. Fix: connect stages with blocking
   `(os/pipe :WR)`, keep `:out :pipe` only on the last stage (janet drains
   it via ev). 2-stage 0/100, 3-stage 0/60, 5000-line stress 0/60.
   Un-guarded wordsplit 22.18/22.19.

2. **janet multi-pattern case → catch-all** (4b0e0f6, P1). `a|b) …`
   emitted `_` (matched everything). Fixed with a janet `match` guard
   alternation `(_casev (or (= _casev "a") …))`. Regression gate cf.c5b.
   Found + filed the separate `bug-janet-case-glob` (`h*` emitted as an
   exact literal).

3. **janet argv trio (DD#60)** (0f36cdf). `(dyn :args)` = [script, ARGS…]
   (matches C argv[0]=progname). `$(argc)`/`$(arg N)`/`for x in $(argv N)`
   (clamped)/`array a=$(argv N)`. argv.sh fully green under janet. **Bonus
   pre-existing bug**: `for x in @a` emitted `(each x [a])` — iterating the
   array as ONE value — for every array; fixed via `janet_arg_splat`
   splice. Added a janet `join` handler. Net −24 janet (argv + cascade in
   control_flow/wordsplit/misc).

4. **&&/|| chains branch on exit status, not value-truthiness**
   (fa9da7c). janet branched on the left block's janet VALUE (a successful
   print → nil → falsy → `||` wrongly fired). Rewrote to branch on
   `__last_exit`; made EVERY command set the status var. c: `emit_echo`
   left `__exit_status` stale so `echo X && cmd` skipped `cmd` — Echo now
   sets it. Un-guarded 62.8-62.10 + added 62.11.

5. **DD#55 (A) external-command abort** (bc65140, PARTIAL). A bare
   (unchecked) external that exits nonzero aborts with that status, both
   backends. c threads the SSA `checked` flag + routes the abort THROUGH
   the END block (new `Emit.end_block`/`abort_target`, fixing a 56.3
   regression). janet re-derives checked-ness structurally
   (`EmitCtx.checked`: false in `-`-suppressed / &&-|| left positions).
   55.1 green all modes; suppress/if/|| catch all correctly do NOT abort.
   Remaining (pipefail+per-segment suppression, c cmdsub, descriptive
   messages, janet pipeline, janet-diagnostic decision) filed as
   `dd55-abort-channel-remaining`.

6. **wordsplit 39.10/22.12** (2b06810, 381512a). The `[[ s =~ pat ]]` test
   operator was unimplemented in BOTH oracle backends (emit_c TODO; emit_janet
   undefined `(=~ …)`) — only the `match S P` builtin worked. Implemented
   `=~`: c → `sash_regex_match`, janet → `sash-regex matchq`. Fixed 39.10
   (`\w`) plus other `=~` tests (c −4, janet −4). `echo | read` (22.12) is
   the lastpipe inline-pipe (c crashes on an invalid fd, janet spawns
   `read`) — guarded to native, filed `echo-read-inline-pipe-cjanet`.

## Key learnings

- **janet os/pipe/os-spawn pipes are non-blocking by default**; use
  `(os/pipe :WR)` for blocking inter-stage pipes handed to children.
- **emit_janet is IR-based** (no SSA `checked` flag) — the DD#55 abort and
  &&/|| status had to re-derive checked-ness structurally via a context
  flag, whereas emit_c (SSA-based) reads the flag directly.
- **the `checked` SSA flag** already encodes condition/suppress/END
  unchecked-ness; threading it into emit_c gave correct if/||/`-` gating
  for free.
- pipefail-on-non-last needs **per-stage `-` suppression**, which the
  backend pipeline-stage tuples don't carry — deferred.

## Followups filed
- `bug-janet-case-glob` (P2)
- `dd55-abort-channel-remaining` (P2)
- `echo-read-inline-pipe-cjanet` (P3)
