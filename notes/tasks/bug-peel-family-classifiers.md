---
title: "Bug: `$(- intrinsic …)`, a nested `keys` and a redirected predicate call skip classifiers that do not peel — ICEs, false SB5050, skipped checks"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "every program in the table is accepted with its control's behaviour or rejected with its control's diagnostic, in both backends — none panics and none prints a wrong value"
  - "one shared peel (or Suppressed/Redirected as statement attributes) serves every classifier named below; no new private peel"
  - "specs pin every row in both backends"
  - "B121: a redirected predicate call (y05, y06, y12, y18, y19) is SB4037 like its unredirected control (y24, y25, y20, y21), in both backends; its Suppressed half (y22, y23) follows the suppression rulings"
  - "batch-531-findings.md ticks B27, B43–B48, B50 and B121, and ticks B42 together with bug-reserved-names-in-arithmetic (which owns B42's `len HOME` half)"
---

## Found

2026-10-27, while porting F1+F4 (batch-531 findings B27, B42–B48 and B50,
journal 537). F1+F4 ported B25 and B31, where a taint classifier did not
peel `-`. These are the open siblings, which the branch did not fix either.
They are **pre-existing**: every result below is the same at `8e28c61`
(before F1+F4) and with F1+F4, in both backends unless noted. F1+F4 does not
modify the functions involved.

| Finding | Program | Result | Control (no `-`, or not nested) |
|---|---|---|---|
| B27/B44 | `map m` / `m[a]=1` / `- keys m k` / `echo $(join k)` | SB5050 1:1, `m` never read | without `-`: prints `a` |
| B27/B44 | `map m=([a]=1)` / `if /bin/true { keys m k; echo "${k[0]}" }` | SB5050 | at top level: prints `a` |
| B27/B44 | `map m=([a]=1)` / `/bin/true && keys m k` / `echo "${k[0]}"` | SB5050 | at top level: prints `a` |
| B42 | `array a` / `a[0]=1` / `x=$(- len a)` / `echo $x` | SB5050, `a` never read | prints `1` |
| B42 | `array a=(1 2)` / `echo "$(- len a)"` | SB5050 | prints `2` |
| B43 | `x=$(- len nosuch)` / `echo $x` | native: compiler panics at `codegen/types.rs:285` (`find_array_slot`, "ICE: undefined arrlen: nosuch"); C: compiles and prints `4294939787` | `if [[ $(len nosuch) == 0 ]] { … }`: SB5039 |
| B45 | `array a` / `a[0]=1` / `str s=,` / `x=$(- join a $s)` | panics at `transform/expr_handlers.rs:443` (`join_intrinsic`: "join separator literal guaranteed by validate::flag_join_separator") | SB5074 |
| B45 | `x=$(- join)` | panics at `expr_handlers.rs:451` ("join arity guaranteed by validate::check_join_arity") | SB5040 |
| B45 | `array a` / `a[0]=1` / `x=$(- join a , extra)` | panics at `expr_handlers.rs:451` | SB5040 |
| B46 | `array a` / `a[0]=1` / `x=$(- len @a)` | panics at `transform/expressions.rs:498` (`literal_name_of`: "a non-literal expression reached literal_name_of") | SB5062 |
| B47 | `str d=z` / `x=$(- arg 5 "$d")` / `echo "${x:!}"` | prints `z` | SB4070: "an `$(arg N DEFAULT)` default is a literal…" |
| B47 | `x=$(- arg -1)` / `echo "${x:!}"` | compiles; at run time "sash: argv index out of range (requested -1, have 0..0)", rc 1 | SB5038: "`arg` index must not be negative — DD#60…" |
| B48 | `map m=([a]=1); x=$(- keys m); echo "$x"` | panics at `transform/stmt_handlers.rs:612` (`handle_keys`: "SB5073/SB4045 reject every `keys` statement but `keys m k`") | SB5025 |
| B48 | `map m=([a]=1)` / `for k in $(- keys m) { echo "$k" }` | SB5009 2:10 | prints `a` |
| B48 | `for a in $(- argv 1) { echo "${a:!}" }` | SB5009 1:10 | compiles and runs |
| B50 | `x=$(- sub abc b '\q')` / `echo $x` | prints `a\qc` | SB4033 |

`/` separates lines of the program. Line numbers of the panics are at
journal 537's commit; the `codegen/types.rs` panic exits 1, the others 101.

B49 stays with F5, where B53's rewrite carries it. B42's other half, `len
HOME`, is `bug-reserved-names-in-arithmetic`.

The branch `taint-walker-redesign` (e98fca8) has a partial B48 fix:
`for_head_keys` and `argv_splat_start` both read through
`ir::single_value_command`, which there peels through `value_command` (H063,
which moved to F5 with B49), and a unit test,
`an_argv_slice_does_not_discard_a_substitution_body_or_wrapper`, pins the
second. It is ledger rows H064, H065, H191 and H197 in `batch-531-hunks.tsv`,
dropped from the port because the iteration still draws SB5009 on the branch
(validate's splat recognisers do not peel). Use it as a reference, not a
patch; if F5 has landed `value_command` by then, reuse it.

Probes: `files/port-f1f4-probes.tar.gz`, directory `peel/` (b38, b42, b48,
b48q, b50, it, kr, len; controls in ctl/ and ctl2/), and `fin2/` (i2c).

## Cause (read, not yet confirmed by a fix)

Each check matches on the statement inside a capture, or on a statement's
kind, and none peels `StmtKind::Suppressed`:

- validate.rs `collect_value_fn_operands` (B27/B44) also does not recurse
  into nested statements, so a `keys` inside `if` or after `&&` is not a
  read of `m`.
- validate.rs `len_cmdsub_operand` (B42, B43), `join_call` (B45),
  `flag_operand_spelling` (B46), `flag_arg_builtin` (B47),
  `check_for_word` (B48's `for` rows) and `check_replacement_in_stmt` (B50).
- The lowering trusts those checks: `handle_keys`, `join_intrinsic` and
  `literal_name_of` panic, and `find_array_slot` panics on an array that does
  not exist.

Journal 495 ("five classifiers, one wrapper") recorded six earlier bugs of
this shape. `arg-negative-index-compile-reject` (done) fixed the unsuppressed
form of the `arg -1` row.

The `$?`/latch checker (`check_dq_stmt`) does not peel a suppressed compound
either. It judges every read inside `- if …` against the enclosing statement's
state. That is `bug-sb5012-sb5094-nested-body-flow`, together with the same
fault for capture, reader-loop and negated bodies.

## Fix direction

One peel authority, called by every classifier above. Or make
Suppressed/Redirected statement attributes rather than wrapper nodes, so
no classifier can forget them (the refinement F1's plan names). Each panic
above names the rule that should have fired, and after the fix each program
should get that rule's diagnostic, as its control does.

Q5 in `q-suppression-and-stage-residuals` is now settled (DG,
2026-11-08; DD70): result discard stays legal, with a separate error for
known pointless computation. `reject-pointless-statements` owns that
check and the affected advice/specs. Effectful or uncertain forms still
need this task's correct shared classifiers; rejecting a result-only
fixture is not proof that its underlying peel defect was repaired.

## B121: a redirected predicate call escapes SB4037

Found 2026-10-28 while porting F2. `f` is `fn f { echo x }`, a void
function with no `return`. Each program compiles, where its unredirected
control is SB4037 ("function `f` is used as a predicate but does not
`return` on every path"). Measured the same on base 1309385, the branch
e98fca8 and hand-parser 899179d, in both backends.

| Id | Program | Result | Control |
|---|---|---|---|
| y05 | `if f >/nonexistent/d/x { echo t } else { echo "e $?" }` then `echo after` | `e 1`, `after` | y24 `if f { … }`: SB4037 1:1 |
| y12 | `if f >/tmp/sash_y12 { echo t } else { echo e }` | `t` | y24 |
| y06 | `f >/nonexistent/d/x \|\| echo "h $?"` then `echo after` | `h 1`, `after` | y25 `f \|\| echo h`: SB4037 1:1 |
| y18 | `if ! f >/tmp/sash_y18 { echo t } else { echo e }` then `echo after` | `e`, `after` | y20 `if ! f { … }`: SB4037 1:1 |
| y19 | `if /bin/true && f >/tmp/sash_y19 { echo t } else { echo e }` then `echo after` | `t`, `after` | y21 `if /bin/true && f { … }`: SB4037 1:1 |

Cause: `validate.rs`'s `mark_predicate_operand` names a `Command`, a
one-command negated `Pipeline` and an `&&`/`||` `List`; its `_ => {}` drops
`StmtKind::Redirected` and `StmtKind::Suppressed`, so the call is never
marked as a predicate. Its feeders (`collect_predicate_uses_stmt`,
`mark_condition_predicate`) and the reporter
(`report_non_returning_predicates`) are not at fault. It is this family's
shape: one classifier that does not peel.

The Suppressed half is held. `if - f { … }` (y22) and
`if /bin/true && - f { … }` (y23) compile and print `x`, `t`, `after` in
both backends. What they should do depends on Q3 of
`q-suppression-and-stage-residuals` (`-` where the status is consumed) and
Q1/Q2 of `q-suppression-on-user-functions`. Fix direction: peel
`Redirected` now, through this task's shared peel; leave `Suppressed` to
those rulings.

(2026-10-30) Q1 and Q2 are ruled. A `-` directly on a call of a void
function is SB4076, implemented by F6 (journal 547). y22's and y23's f
prints `x` and has no `return`, so it is void, and F6 rejects both.
That does not fix the redirected calls. `if - status_f`, a `-` on a
call whose status the condition consumes, stays with
`q-suppression-and-stage-residuals` Q3.

**Closeout recheck, 2026-11-05 (`09a5bd49`, journal 570).** With the same
void `f`, all four distinct redirected forms still compile in native and C:
`if f >file`, `f >file || ...`, `if ! f >file` and
`if /bin/true && f >file`. Their outputs are respectively `yes`, empty,
`no` and `yes`, all exit 0, with a writable file in the harness's temporary
directory. The unredirected `if f` control gets SB4037.
`mark_predicate_operand` still has no `Redirected` arm
(`validate.rs:3937-3954` at this commit). B121 stays open here; neither
F6's suppressed-void repair nor the later captured-int-predicate repair
closes this wrapper gap. Sources and full mode xtraces are in
`files/port-closeout-probes.tar.gz`.
