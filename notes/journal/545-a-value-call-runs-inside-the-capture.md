# 545 - A value call runs inside the capture

2026-10-29. F5 unit D of the batch-531 port (Order item 12; journal 542
has the four-unit split): B71 and B74. With this unit, all of F5's code
has landed.

## The bugs

An `-> int` function called with value syntax is inlined where the call
stands (DD58 Option C). The value syntaxes are `$(f)`, `(f)` in
arithmetic, and a bare `f` in arithmetic. Its body writes wherever that
statement's output goes, and inside a capture that is the capture. Two of
the capture fast paths did not account for this.

### B71: the purity walk did not see value calls

capture_purity chooses how a capture site holds its stdout: a userspace
buffer (`sash_capture_start_buf`) or a memfd. The buffer sees only
`sash_write`, so a forked external's output goes past it to the real
fd 1. The walk judged a site's own statements and the bodies of
bare-called functions, but it did not look at value calls. A value call
whose body ran an external therefore left the site buffered.

The probes use `fn count -> int { /bin/echo ext; return 7 }` and
`str s=$(outer); echo "[$s]"`; `⏎` is a newline inside `$s`. Both backends
agreed, before and after:

| Probe | Position | Before | Now |
|---|---|---|---|
| h1 | `fn outer { int n=$(count); echo "$n" }` | `ext`, then `[7]` | `[ext⏎7]` |
| h2 | anonymous: `str s=$(int n=$(count); echo "$n")` | `ext`, then `[7]` | `[ext⏎7]` |
| h3 | `int n=$(( (count) ))` in outer | `ext`, then `[7]` | `[ext⏎7]` |
| h6 | `if (( (count) > 0 )) { echo y }` in outer | `ext`, then `[y]` | `[ext⏎y]` |
| h7 | `echo "v=$(count)"` in outer | `ext`, then `[v=7]` | `[ext⏎v=7]` |
| v1 | `int n=$(( count + 1 ))` in outer | `ext`, then `[8]` | `[ext⏎8]` |

h1b replaces the external with the builtin `echo ext`, and it already
printed `[ext⏎7]`. An in-process write reaches the buffer; only a fork
escapes it.

### B74: the value path evaluated arguments outside the capture

handle_command_substitution has two paths for a capture with one command:
- The value path (`__strcap_<fn>`, `FuncCall`, `CmdSubst`) evaluates the
  arguments first, where the capture stands, and only then opens the
  capture.
- The body path (`cmdsub_body_value` → `lift_int_func_calls` →
  `capture_body_into`) opens the capture first and lowers the command
  inside it.

On the value path, an argument that runs a value call wrote its output
before the capture opened. The probes use `fn g -> int { echo G; return 1 }`
and `fn h { echo "h:$1" }`, in both backends:

| Probe | Capture | Before | Now |
|---|---|---|---|
| g1 | `str s=$(h $(( (g) )))` | `G`, then `[h:1]` | `[G⏎h:1]` |
| g4 | `str s=$(h $(g))` | `G`, then `[h:1]` | `[G⏎h:1]` |
| g8 | `str s=$(- h $(g))` | `G`, then `[h:1]` | `[G⏎h:1]` |

The redirected twin, `$(h $(( (g) )) 2>/dev/null)`, was already a body
and always kept `G`.

g2 and g3 make g's write external (`/bin/echo G`). g2 is g1's capture;
g3 is `$(echo $(( (g) )))`. They printed `G`, then `[h:1]` or `[1]`, and
they need both fixes. B74 moves the call inside the capture, and B71 must
then judge the site impure, or the external writes past the buffer
another way.

g8 is the suppressed form, and it changed with no code of its own. The
`Suppressed` test in handle_command_substitution always sent it to the
body carrier. Then `suppressed_capture_value` (functions.rs:338, from
coverage 530) removed the `-` and re-entered handle_command_substitution
with the bare `h $(g)` inside a SuppressionScope. That re-entry took the
value path. B74's disjunct now catches the re-entry, so the body is
lowered inside the capture, still in the scope. No spec pins g8, by DG's
2026-10-28 rule that no spec may pin `-` on a user function.

## The port

These come from the branch. The decide-and-report choices are listed
after them.
- resolve.rs: the value-call walker from H141:
  - `walk_value_calls` covers a statement's own expressions and an
    `(( ))` test;
  - `walk_expr_value_calls` covers one argument;
  - `evaluates_args_in_place` holds the shared predicate: a value
    function, or an intrinsic whose arithmetic denotation is `Int` or
    `Str`;
  - `arith_value_calls` handles a sexpr `(g …)` or a bare `g`, and does
    not descend into a `len`/`strlen` operand, which is a name;
  - `substitution_value_calls` and `value_calls_at`. The latter is total
    over `ExprKind`.
- capture_purity.rs is the branch's file verbatim. It has the `Purity`
  walker with `value_calls_are_pure`, and `capture_body_is_pure` takes the
  value-function set.
- expr_handlers.rs: the disjunct `|| capture_args_run_value_calls(…)`, and
  the function, which declines when the head evaluates its arguments in
  place.
- stmt_handlers.rs: `capture_func_into` and `capture_body_into` pass
  `&state.function_int_typed` to the purity check.
- docs/GUIDE.md: one sentence in the capture-paths section.

Decided and reported (DG may veto):
- **H141 is reduced to the name walk.** `Evaluation`, `walk_evaluations`,
  `walk_expr_evaluations` and `value_call_name` are not ported. Their only
  consumer is the branch walker's `taint_evaluation` (H264, superseded by
  W1). Their unread payloads would also fail `clippy -D warnings`. The
  ledger note records the split.
- **One arithmetic walker.** The branch recursed through arithmetic in a
  private copy. Here, `ir::walk_arith_nodes` delegates to a new
  `walk_arith_nodes_pruned`, whose callback answers whether to descend,
  and the value-call walk uses that. Unit C had just merged three private
  walkers into one (journal 544), and this keeps it one.
- **The B71 specs run under `--no-taint`.** They compile clean today,
  because a user function's capture is clean (DD24). T1 (W2) will reject
  them, as the branch already does for h1 and h2 (SB5020). They pin where
  the bytes go, so that verdict is not theirs to pin.
- **Beyond the branch:** cap.34, which covers the bare-name position, and
  the unit test `intrinsic_operands_are_names_not_value_calls`.
- **No DD addendum.** DD58 Option C already says a value call runs where it
  stands; the two fast paths now follow it. The GUIDE sentence is the
  architecture note.

## Probes

The probes are in /tmp/port/f5/ud and are archived at the F5 wrap-up.
"Before" is this tree at f679cb2 (unit C), and each probe ran in both
backends. The rows in the tables above changed. These did not:
- g5, `$(echo x; int v=$(k $(g)); echo "$v")` with
  `fn k -> int { echo "K$1"; return 2 }`, prints `[x⏎G⏎K1⏎2]`. `k` is a
  value call, so `v` is its return value, and both writes land in the
  enclosing capture in order. This is cap.33, the control.
- g6, `$(echo x; echo $(( (g) )))`, prints `[x⏎G⏎1]`. A body with two
  statements was always a body.
- g7, `$(true; h $(g))`, gives SB5020 on both trees. `true` has no
  `BuiltinCmd` arm (only `:` has one, ssa.rs:195), so it resolves as an
  external and taints the capture.
- g9, h4 and h5, a `readline` from a pipe, give SB5020. r4 also gives
  SB5020.
- r1 and r2, `$(count 2>/dev/null)`, and w3, `fn wrap { count }`, give
  SB4016.
- r3, `int n=$(- count)` at top level, prints `ext⏎n=7`.
- w1, w2 and w4 are top-level `echo "v=$(count)"`, `str s="$(count)"` and
  `int n=$(( (count) + 1 ))`. No capture encloses them.
- v2, `$(( (strlen count) ))`, prints `[5]` before and after. `count` is
  the operand of `strlen` there, not a call. Only the unit test can see
  the difference (stub f).

## Specs

misc.sh cap.24-cap.34 (plan 112 → 123). Each spec asserts the exact
output, `rc=0` and an empty stderr:

| Spec | Probe | Flags |
|---|---|---|
| cap.24 | h1 | `--no-taint` |
| cap.25 | h2 | `--no-taint` |
| cap.26 | h3 | `--no-taint` |
| cap.27 | h6 | `--no-taint` |
| cap.28 | h7 | `--no-taint` |
| cap.34 | v1 | `--no-taint` |
| cap.29 | g1 | |
| cap.30 | g4 | |
| cap.31 | g3 with the builtin `echo G` (B74 alone) | |
| cap.32 | g2 (both fixes) | `--no-taint` |
| cap.33 | g5 (control) | |

The negative control used the pre-F5 binary (2320434) in both modes.
Exactly cap.24-cap.32 and cap.34 were red, and cap.33 passed, as a control
should.

Reversion stubs, each identical in both modes:

| Stub | Edit | Red |
|---|---|---|
| (a) | `value_calls_are_pure` returns true | cap.24-28, cap.34, cap.32 |
| (b) | the disjunct is disabled | cap.29-32 |
| (c) | the `evaluates_args_in_place` decline is deleted | cap.33 |
| (d) | `walk_value_calls` loses its ArithTest arm | cap.27 |
| (e) | `arith_value_calls` answers `FuncCall` only | cap.34 |
| (f) | the `len`/`strlen` prune is removed | no spec; the unit test fails |

Every arm the unit adds turns something red. Stub (f) is the reason the
unit test exists: without the prune, a site is judged impure for no
reason, and no output can show it. The unit test is in capture_purity's
`api_contract_tests`, which brings the unit count to 238.

## The gate

`NEW=22 mk gate` passed (rc 0):
- static checks were clean, and the plan sums are native 5264 + c 5043;
- the sweep covered rows 6502668-6512974: 10307, which is 10294 pass +
  13 skip;
- the first-ever ids were exactly cap.24-cap.34 in both modes;
- there were 0 flips, 0 retired and 0 ghosts;
- liveness was 260.

After the gate passed, the pre-commit complexity gate rejected
`arith_value_calls` at cc 6 / cog 5. Its nested `if is_value_fn(name)`
became a match guard, which gives the same arms and the same answers at
cc 5 / cog 2, inside the cog ≤ 2 exemption. Because only that refactor
changed, the checks re-run on the committed tree were `mk test` (rc 0) and
misc.sh, which holds cap.24-cap.34, in both modes: native 123/123, and c
113 with 10 skipped. The full sweep was not re-run.

## Ledger

Nine rows are now ported:
- H141, reduced to the name walk, as its note says;
- H170-H173, capture_purity;
- H202 and H203, expr_handlers;
- H237 and H240, stmt_handlers.

No F5 row is pending. The ledger now stands at 211 ported, 51 pending,
62 superseded, 5 dropped and 1 reimplemented. B71 and B74 are ticked.

## Backlog

This unit files nothing new. F5's three questions follow at the wrap-up:
capture-hoisting order, B34 (`exec` in a capture), and whether captures
may write outer names.

## Narrative

B71 and B74 are one fact seen from two places. A value call is a place
where a body runs. The purity walk did not count it, so it was wrong about
what the capture would see. The value path did not count arguments as
such a place, so it ran them before the capture opened. Both now ask the
same walker, and a single predicate, `evaluates_args_in_place`, draws the
same line in both: a value call evaluates in place, and anything else is
a capture. The suppressed form came along unasked, because coverage 530
had already routed it back through the same dispatcher.
