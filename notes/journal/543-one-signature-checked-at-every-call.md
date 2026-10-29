# 543 - One signature, checked at every call

2026-10-29. F5 unit B of the batch-531 port (Order item 12; journal 542
has the four-unit split): int-function arity (B56) and the signature scan
(B53), with B49, which F1+F4 handed to this unit (journal 537).

## The bugs

DD81 infers one signature per function from how its body reads `args`,
and checks every call against it: SB5042 for too few arguments, SB5043 for
too many. Both halves were broken.
- **B56** (DD58, DD81). An `-> int` function was never checked, because
  `inline_int_func` had no arity check. With
  `fn f -> int { return $(( $1 + 1 )) }`, `echo $(( (f) ))` printed 1 and
  `$(( (f 1 2 3) ))` printed 2 (sig/l4, l6). The same held for
  `int n=$(f)`, `if (( (f) > 0 ))` and a body of `(strlen $1)` (l7, l9,
  x2). The branch put its check in `inline_int_func`. F13 made the `fn_`
  template loop, so an int function's self tail call now lowers through
  `lower_int_tail_call` and would have escaped that check:
  `return $(( (f) ))` and `return $(( (f $1-1 9) ))` inside `f` printed 0
  (t3, t4; F13's t5).
- **B53** (no DD). `analyze_arity` used its own forked copy of the body
  walk, which ended in `_ => {}`. That copy missed a positional read in:
  - an assignment index, `m[$1]=v` (a1);
  - a dispatch pattern, `[[ x == $1 { … } || { … } ]]` (c1);
  - a subscript under a default, `${m[$1]:-d}` (p1, p2), including inside
    `$(( ))` (p4);
  - a redirect target, `>${1:?no target}` or `>${1:-default}` (r4, r5);
  - a map literal, `map m=([a]="$1")` (s4) and `map m=([a]=$(join args))`
    (s5);
  - `$(<$1)` (s3, visible only under `--no-taint`, because by default
    SB5020 fires first).

  A function that read `$1` only in one of those places was taken to have
  no arguments, so every call with arguments drew a false SB5043.
- **B49** (DD55). `cmdsub_reads_whole_args` matched only a bare `join`
  command. As a result `$(- join args)` did not make the function
  variadic, and `fn f { str s=$(- join args); echo "[$s]" }; f a b c` was
  SB5043 (j1, wa2).

## The port

- `analyze_arity` runs the shared traversal: `ir::walk_scoped_stmts`
  reaches every nested body, `$( … )` bodies included, and
  `ir::walk_own_exprs` hands over each statement's own expression trees.
  It records reads only in its own scope (`in_fn.is_empty()`), because a
  nested function has its own `args`. `sig_scan_own` records one
  statement's contribution: its expressions via `sig_record_expr`, which
  is total over `ExprKind`; DD81's external `@args` rule
  (`splats_args_into_external`); and a `(( … ))` statement's bare `$N`
  reads.
- The private walk's eleven `sig_scan_*` functions are gone. Two are
  replaced by recorders that look at a single node and never recurse:
  `sig_scan_expr` by `sig_record_expr`, and `sig_scan_arith` by
  `sig_record_arith_names`. The shared walk delivers each `${…}` node by
  itself, so the second recorder's `Expansion` arm does nothing.
- `ir::value_command` is split out of `single_value_command`. It peels a
  statement down to its value command, so `cmdsub_reads_whole_args` finds
  `join` under `-`. Its doc points at `bug-peel-family-classifiers`.
- `keys` is dropped from `cmdsub_reads_whole_args`. `$(keys args)` is
  SB4030 (`75.100`), so it could never have reached the scan.
- `inline_int_func` calls `check_callee_arity` after
  `reject_recursive_call`, as the branch does. `lower_int_tail_call` calls
  it too, on the plain self-tail path. That second call goes beyond the
  branch because F13 introduced the tail path, so it has no ledger row.

Suppression is not touched. `- join` is an intrinsic, so B49 does not
cement `-` on user functions (DG, 2026-10-28). `inline_int_func` also
lowers the one suppressed self-tail entry, and its check is unconditional,
so that entry is checked too. No spec pins this, because doing so would
put `-` on a user function.

## Probes

Pre-edit baseline: the unit A binary, 49 probes in /tmp/port/f5/ub/sig.
The baseline file truncates messages, so 13 rows differ only in text and
keep the same verdict. Real changes: 21, each in the predicted direction.

Now compile and run. Each was a false SB5043 ("takes exactly 0 arguments
but was called with N"):

| Probe | Form | Prints |
|---|---|---|
| a1 | `m[$1]=v` | v |
| c1 | `[[ x == $1 { … } ]]` | m |
| p1 | `${m[$1]:-d}` | v |
| p2 | `${a[$1]:-d}`, indexed | y |
| p4 | `$(( ${a[$1]:-0} + 1 ))` | 6 |
| r4 | `>${1:?no target}` | done |
| r5 | `>${1:-default}` | done |
| s4 | `map m=([a]="$1")` | [x] |
| s5 | `map m=([a]=$(join args))`, called with 2 | [x y] |
| j1 | `str s=$(- join args)`, called with 3 | [a b c] |
| wa2 | `echo $(- join args)` | a b c |

Now rejected. Each used to compile and print the value in brackets:

| Probe | Form | Now |
|---|---|---|
| l4 | `$(( (f) ))` [1] | SB5042 at 2:1 |
| l6 | `$(( (f 1 2 3) ))` [2] | SB5043 at 2:1 |
| l7 | `int n=$(f)` [1] | SB5042 at 2:1 |
| l8 | `int n=$(f 1 2)` [2] | SB5043 at 2:1 |
| l9 | `if (( (f) > 0 ))` [pos] | SB5042 at 2:4 |
| x2 | body `(strlen $1)`, `(f)` [0] | SB5042 at 2:1 |
| t3 | self tail `(f)` [0] | SB5042 at 1:33 |
| t4 | self tail `(f $1-1 9)` [0] | SB5043 at 1:33 |
| ta5 | too-few self tail under `(f 3)` [0] | SB5042 at 1:33 |
| ta6 | `int r=$(( (f) ))` [1] | SB5042 at 2:1 |

Unchanged controls:
- j2, j5, j7, p3, r6, x1 and x4 run as before.
- The existing arity errors stay as they were: j8 and l1 (SB5043), l3 and
  l5 (SB5042), and the string self tails t1 and t2 (SB5042, SB5043 at
  1:33).
- Rejected spellings keep their codes:
  - SB4030: j3, j4, j6, j10, and r1-r3 (a redirect target that may be
    empty);
  - SB5025: j9;
  - SB5026: j11;
  - SB5066: s1, s2;
  - SB5020: s3;
  - SB5031: x3;
  - SB5037: l2.

Two rows are not what DD80 wants. p2 and p4 index an *indexed* array with
`$1`. DD80 makes that SB4030; it compiles only because of
`bug-subscript-key-check-skips-positions` (its ie1 and ie4). The specs
therefore use map keys, so they will not flip when that P1 lands.

## Specs

functions.sh 81.47-81.61 (plan 245):
- B53 and B49 positives. 81.47 a1, 81.48 c1, 81.49 p1, 81.50 the map
  twin of p4, 81.51 r4, 81.52 s4, 81.53 s5, 81.54 j1.
- B56 rejections. 81.55 l4, 81.56 l6, 81.57 l7, 81.58 l9, 81.59 x2,
  81.60 t3, 81.61 t4.

Left out:
- r5, wa2, l8, ta5 and ta6: each repeats a channel that a spec already
  pins (ta5 is t3's self tail with an `int r=` call site);
- p2 and p4: indexed arrays, see above.

Negative control on the pre-port binary (F3, 2320434), both modes: all 15
are red. B53's positives fail there with the false SB5043, as they did on
the unit A binary in the baseline. Reversion stubs, each red in both
modes:

| Stub | Change | Red |
|---|---|---|
| g | no check in `inline_int_func` | 81.55-81.59 |
| h | no check in `lower_int_tail_call` | 81.60, 81.61 |
| i | `cmdsub_reads_whole_args` matches a plain command again | 81.54 |

81.47-81.53 have no per-channel stub. Channel coverage is a property of
the shared walk, not of one arm, and the negative control pins all seven.

## The gate

`NEW=30 mk gate`: GATE PASS. Rows 6482108-6492382, 10275 = 10262 pass +
13 skip (5248 native, 5027 C), 30 first-ever, 0 flips, 0 retired, 0
ghosts, liveness 260. The prediction is 30, not 15, because a first-ever
id is per mode.

## Ledger

Rows ported:
- H063: `ir::value_command`;
- H206: now both halves (unit A took `reject_recursive_call`);
- H208-H214: `analyze_arity`, `sig_scan_own`, `sig_record_expr`, the
  removed `sig_scan_test`, and the `cmdsub_reads_whole_args` and
  `sig_record_arith_names` edits.

Findings B49, B53 and B56 are ticked.

## Backlog

Nothing new. The indexed-array rows (p2, p4) are already
`bug-subscript-key-check-skips-positions`'s ie1 and ie4.

## Narrative

DD81's one-signature rule has two halves: infer the signature once, and
check every call against it. The bugs broke both. B53 inferred the
signature from a walk that had drifted from the shared one, and B56
skipped the check on a whole class of calls. Inference now uses the walk
every other pass uses. The check runs on every call path, including the
loop that F13 gave the self tail call.
