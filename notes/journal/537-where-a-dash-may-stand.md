# 537 - Where a dash may stand

2026-10-26/27. F1+F4 is the fifth family ported, and the largest. It settles
where `-`, `!` and `&` may stand, what may run as a pipeline stage or a
background job, and how a classifier sees through the wrappers a statement
can carry. Two of DG's remarks on 2026-10-26 shaped it. On reserved words:
"I don't see reserved words being meaningful within a pipeline or in
combination with &". And, after the discussion that followed: "yes to
SB4019, broaden the scope for the touched functions". I read the second as:
every known finding inside a function the port modifies is fixed now, and a
finding anywhere else goes to the backlog under the 2026-10-25 rule
(decide-and-report).

## The bugs

Three shapes, one family.

**A wrapper that a classifier forgot.** `-`, `!` and a redirect are
statement nodes (`Suppressed`, `Negated`, `Redirected`) around the command
they modify, so every classifier that asks "what is this statement?" has to
look through them. The taint source classifier did not. Spec 24.21 is B25,
the CRITICAL:

```
- /bin/cat b25.txt | readline x
echo "got $x"
```

At `8e28c61` this compiled and printed `got secret`: the `-` hid `/bin/cat`
from the classifier, so `x` came out clean. B31 is the converse. A `-` or
`!` on the reader stage hid a clean producer, so `echo hi | - readline x`
drew a false SB5020. The branch listed eleven more classifiers with the
same fault (B27, B42-B50).

**A state effect where its effect is lost.** A pipeline stage and a
background job each run in a forked child. A builtin that acts on the
shell's own process (`return`, a write, `cd`, `read`) does nothing useful
there, or worse. pf.35:

```
fn g { /bin/echo x | return 3 }
g
echo "st=$?"
```

compiled and died at run time with "bug: bare command passed to sash_exec"
(B12), as did every in-process builtin but `echo`, `write` and `:`, in any
stage. `fn g { return 3 & }` returned into its caller's code in the child,
so `done` printed twice (B11, 55.116). `x=b &` and `(( n = 1 )) &` lost the
write.

**Grammar that took too much or too little.**
- `a | b &` drew a false SB4022, though DD#55 says it is legal (B23).
- A `-` operand swallowed the rest of its list, so `/bin/true && - /bin/false
  || /bin/echo x` slipped past DD#95's SB4053 (B24).
- A background job could end a condition (B26).
- `int n=1 | cat` was SB4014 "expected variable name in declaration" at 1:1
  (B28).
- `while - { … } { … }` sidestepped SB4027 (B112).
- `- ! cmd` compiled, though the `!` can change nothing the `-` has not
  already discarded.
- An empty keyword condition (`if then`) tested whatever status the previous
  command left.
- A dispatch reached boolean positions that DD#66 forbids.
- `return -1` was SB5011 "command '-1' not found".
- One the port found itself, on base and branch alike: a statement-level `-`
  folded its statement a second time, so `- a & && b` ran `b` (55.137), and
  `- a & | b` reached an ICE.

## The fix

**One peel authority.** `Statement::stage_marks` (`ir.rs`) peels `-` and `!`
and reports which it found; `Statement::stage_body` also peels a redirect.
Every classifier the port touches asks one of them: the taint source
classifier (B25), B31's reader test in `taint_stmt_no_read_taint`, the stage
gate, the final-reader test and the transform's final stage. The classifiers
in functions the port does not touch are `bug-peel-family-classifiers`.

**One state-effect classifier.** `ssa::BuiltinCmd::acts_on_its_process` says
whether a builtin acts on the process that runs it. It is total over
`BuiltinCmd`, with no wildcard, so a new builtin must be classified before
the compiler builds. The stage gate and the `&` gate both ask it. They part
in two places. The stage gate exempts a pipeline's final reader, which
DD#122 runs in the parent, so `a | readline x` is legal while `readline x &`
is SB4024. And `exec cmd &` stays legal (the reason first given here was
wrong; see the correction below), while `a | exec b` is SB4022 with a message
of its own.

**The parser** (`parse/parser.rs`):
- `unsuppressible_error` rejects a `-` in front of what it cannot suppress:
  `!` (SB4019, in either order), and `attest`, a declaration, `fn`/`function`
  and `END` (SB4014 at the `-`). `dash_prefixes_command` gains `until` and
  loses `!`.
- `background_fault` decides what `&` may hold. SB4024 for a compound, a
  control word, an assignment or declaration, `(( = ))`, `cd`, a bare
  `exec`, and every builtin `acts_on_its_process` claims. It looks through
  `-` and `!`, into a list's operands and a pipeline's stages.
  `is_background_job` and `background_status_error` give B26's SB4024 for a
  job ending a condition or a return producer.
- The stage gate is SB4022. `reject_statement_form_pipeline_stage` asks
  `stage_fault` about each stage beneath its marks: a statement form, or
  (`in_process_stage_fault`) an in-process builtin as any stage but the
  final reader, and a redirect on the final reader.
  `reject_nonconsuming_final_stage` adds `!` on a `while` final stage.
  `statement_form_label` calls an assignment and a declaration what they
  are (B19). The private `unwrap_stage_wrappers` is gone, and
  `segment_form`, split out of `append_pipeline_stage`, only turns `a | ! b`
  into a `Negated` stage.
- `parse_suppressed` takes a `Reach`. A statement-level `-` takes the
  statement up to its `&` and folds it once; an operand `-` takes one
  operand, so the list operators after it belong to the list (B24).
- `condition_shape_fault` and `missing_condition_error` make an empty keyword
  condition and a braced group anywhere in a condition SB4027, behind `-`
  too (B112). Only a group the user braced counts; the arithmetic comma's
  and C-style `for`'s internal groups do not (`GroupOrigin`).
- `dispatch_in` is the one place a dispatch is recognised. It peels `-`,
  `!`, a return producer and a redirect, and its four callers give SB4028 in
  every boolean position.
- A stray block keyword is SB4072 wherever a command may stand. A block that
  runs into a closer is reported at the closer, and no keyword is judged
  after a parse error.
- `parse_declaration_assignments` stops at `|`, `&`, `&&` and `||`, and at
  `{` in a control-flow header, so a declaration gets an assignment's
  verdicts (B28).
- `binding_fd_word` recognises an fd number glued to the redirect operator
  after it (`2>f`, not `2 >f`), so `2>f` after a compound or a `!` is the
  redirect it is (SB4021, SB4046).
- `parse_return` takes a word as the status when `ir::is_int_sink_literal`
  says an int sink would: an optionally signed decimal within i64.
- The arithmetic comma's group answers as the assignment it is (DD#31).

In `parse/expr.rs`, `rediagnose_stray` turns an SB4072 inside a `$(` that
never closes into SB4057 at the `$(`. Its token scan, `skip_to_cmd_sub_close`,
was split into a loop and a step (`cmd_sub_close_step`) for the complexity
gate.

**The transform** (`transform/stmt_handlers.rs`). `FinalStage` holds the
last stage's body with its `-` and `!` marks. `consumes_as_statement` says
whether the stage lowers as a statement bound to the pipe (a `while`, or a
`read` into an array), and `verdict_joins` whether its status joins the
producers'. A suppressed final reader's does not, so the verdict of
`a | - readline x` is the earlier stages'. `lower` runs a `-` consumer
unchecked and inverts a `!` reader's verdict before it joins.
`check_negation_must_consume` makes an unconsumed terminal `!` SB4018, as a
standalone `! cmd` is. SB4018's advice now says to write `- cmd` without the
`!`; it used to suggest the dead `- ! cmd`.

**The validator.** `taint_stmt_no_read_taint` peels the reader stage's marks
(B31). `is_external_source` classifies `stage_body()` (B25); its head test
moved verbatim into `source_head_execs`, because the touched function was
over the complexity gate.

## What changes

By DD; status.md has the same list.
- **DD#24.** `- source | readline x` taints `x` (B25). A clean producer
  leaves a marked reader clean (B31). An external producer still taints a
  `-` or negated reader.
- **DD#55.** Under `&`, SB4024 for whatever acts on the shell's own process:
  compounds (also behind `!` or in a list), `return`/`break`/`continue`/
  `exit` (B11), assignments and declarations, `(( n = 1 ))`, `cd`, a bare
  `exec`, and the in-process builtins. `a | b &` (B23) and `exec cmd &` are
  legal. A job ending a condition or a return producer is SB4024 (B26). A
  `-` operand or stage takes that operand alone (B24); `- a & && b` is
  SB4014. `- until` parses like `- while`. The addendum corrects the old
  "exec failure … the parent aborts": exec fails in the child.
- **DD#122.** An in-process builtin is SB4022 as a stage in any position
  (B12), and `exec` has its own message. `a | - readline x` and `a | - read
  ar` are legal. A negated final reader is legal where a condition or a
  return producer consumes it. A redirect on the final reader, or `!` on a
  `while` final stage, is SB4022. SB4073 names an `until` loop as `until`.
- **DD#70.** `- !` is SB4019 in both orders. An unconsumed terminal `!` is
  SB4018. `!` and fd tags no longer hide a redirect, a pipe or `&` (SB4046,
  SB4021, SB4022).
- **DD#71 (B28).** A declaration stops at `| & && ||`; in a header, `{` ends
  it. Outside a header `int n=1 { … }` stays SB4014.
- **DD#68.** Empty keyword conditions and `{ }` group conditions are SB4027
  anywhere in a condition (B112). A stray block keyword is SB4072 (it was
  SB5011, or SB5092 for `fn in` plus a call). Clause errors point at their
  clause (`diag-elif-clause-has-no-span`). A stray closer inside an
  unclosed `$(` is SB4057.
- **DD#66.** A dispatch in any boolean position is SB4028
  (`bug-dispatch-in-boolean-positions`).
- **DD#31.** A comma with a write gets an assignment's answers: SB4022,
  SB4046, SB4024. Before or after `&&`/`||` it keeps the compound's SB4023
  until DG rules.
- **DD#74.** `return -1` and `return +1` are values; a number past i64, and
  `-x`, are commands (SB5011). The number used to ICE native and abort C at
  run time.

## Specs and proof

201 new ids per mode. `plan` goes from 509 to 664 in `errors.sh`, 220 to 223
in `functions.sh`, 104 to 138 in `pipe_functions.sh` and 365 to 374 in
`taint.sh`.

| Ids (file) | Pins |
|---|---|
| 24.21-29 (taint) | B25, B31; 24.26-27 an external producer still taints |
| 55.107-115 (errors) | `-` on attest, str/int/const/array/map, fn/function, END |
| 55.116-118 | B11 |
| 55.119-120 | B26, n4 |
| 55.121-128 | `&` on state effects; 55.128 `exec cmd &` is legal |
| 55.129-131 | B24 (55.130 positive) |
| 55.132-136 | `&` looks through `-` |
| 55.137-144 | the double fold (55.143 control, 55.144 positive) |
| 71.1-9 | B28 (71.5 positive; 71.6-8 header `{`; 71.9 control) |
| 74.67-69 (functions) | `return -1`; past i64 is SB5011; `-x` is a command |
| acm.1-12 | DD#31 comma with a write (acm.11-12 the `&&`/`||` hold) |
| bng.13-25 | `-` and `!` |
| cnd.1-24 | conditions (cnd.7 is B112; positives 11, 12, 18) |
| csfx.4-20 | negated group suffixes, fd tags (csfx.19 control; csfx.20 was a parser ICE) |
| pf.35-68 (pipe_functions) | B12, B19 (pf.41), the final reader, B23 (pf.52), SB4018, SB4073 |
| rdr.50-57 | a redirect after a `!`-negated statement |
| skw.1-34 | stray keywords, SB4072 |

None of them comes from the branch, which has no F1 or F4 specs; all are
written from probes. Of the 402 rows, 294 first ran in this port's
development sweeps and 108 were first-ever in the final sweep. None reuses a
retired id. Three existing specs were rewritten in place and keep their ids,
as in F14: e.b70.ndash and bng.5 (now SB4019) and cov.182 (its
`read A | read B` is now SB4022, so it uses `&&`).

The spec set was trimmed by one rule (decide-and-report): keep a spec if it
is the only one some single-point reversion turns red, or if it pins a
failure class on the pre-port binary that no other kept spec in its group
pins.

**Negative control.** On the pre-port binary 179 of the 201 fail, in both
modes. The 22 that pass are each labelled a control, a positive or a hold:
24.26, 24.27, 55.128, 55.130, 55.143, 55.144, 71.5, 71.9, 74.69, acm.11,
acm.12, bng.20, bng.22-24, cnd.11, cnd.12, cnd.18, csfx.19, skw.29, skw.31
and skw.34.

**Reversion stubs.** There are 72, plus a no-change control (base0). Eleven
of them (K1-K7, CL1-CL3 and PF67) pin work done after the first stub run,
and all 72 ran again on the final source. Each reverts one site; the harness
skips a stub whose target text does not occur exactly once, and it skipped
none. Then the five touched spec files (errors, functions, pipe_functions,
taint and coverage) ran in both modes. Every red id below was red in both,
except pipeown.28 under S9b (below).

- **Taint.** S1 (the source classifier reads the raw statement): 24.21,
  24.27, and nine existing specs (60.13, 60.24, 60.32-34, 60.48, 60.49,
  60.51, ad.4). S2 (B31's reader test): 24.22-25, 24.28, 24.29.
- **Stage gate.** S3 (no builtin refused): pf.35-38, 40, 42, 53-60. S3a (the
  non-final message): pf.39, 46. S3b (a redirect on the final reader):
  pf.43, 44. S3e (`exec`'s message): pf.42. S9 (B19's label): 55.121,
  55.122, 55.133, 55.134, pf.41. S9b (the declaration label): 71.3, 71.4,
  and once pipeown.28 in C mode, a harness artifact (below). S21 (`!` on a
  `while` final stage): pf.45, pf.68.
- **`&` gate.** S4 (all of `background_fault`): 55.116-118, 55.121-127,
  55.131-136, 71.4, acm.8, acm.10, csfx.4-8. S4a (statement forms):
  55.121-123, 55.133, 55.134, 71.4, acm.8, acm.10. S4b (commands):
  55.116-118, 55.124-127, 55.131, 55.132, 55.135, 55.136. S4c (compounds):
  csfx.4-8. S4e (pipelines): 55.123, 55.135, 55.136, csfx.4-8. S4f (lists):
  55.131-134, csfx.4, 5. S4g (through `-`/`!`): 55.132-136. S4h (redirects)
  and S4j (bare `exec`): 55.125. S4k (`exec` with a command): 55.125,
  55.128.
- **Conditions and `return`.** S5a (B26): 55.119. S5b (n4): 55.120. S6
  (braced group): bng.21, cnd.5-7. S6b (internal groups count): cnd.11,
  cnd.12, and four existing specs (cov.2, cov.3, cov.48, cov.153). S7 (the
  dispatch test): cnd.8, 9, 19, cov.209. S7d (`dispatch_in`'s `Case` arm):
  cnd.8-10, 13-17, 19-21, cov.209. S7b (the peel): cnd.7. S7c (lists):
  bng.21, cnd.5. S8 (empty keyword condition): cnd.1-4, 22-24, and three
  existing specs (cov.13, cov.207, cov.359). S20: cnd.13.
- **`-` and `!`.** S10 (`unsuppressible_error`): 55.107-115, bng.15, 16, and
  the rewritten bng.5 and e.b70.ndash. S11a (redirect after `!`): acm.7,
  csfx.10, csfx.20, rdr.50-57. S11b (compound redirect): csfx.10, csfx.20;
  csfx.11's comma list is not a compound to `is_compound`, so AC1 and AC5
  pin it instead. S13 (`- until`): bng.25. S17 (`Reach`): 55.129, 55.131,
  bng.13, 14.
- **Declarations and lists.** S12 (a connector ends a declaration): 71.1-4.
  S16 (a stage parses as a list operand, not a statement): bng.18, pf.52.
  S22 is an equivalence control (the old redirect `matches!` list against
  `is_redirect_op`): none.
- **`acts_on_its_process`.** S14a (control words): 55.116-118, pf.35-38.
  S14b (`cd`, `close`, `wait`, `unset`, `signal`): 55.124, 55.131, 55.132,
  pf.40, 53-56. S14c (the reading builtins): 55.126, 55.127, 55.135,
  55.136, pf.39, 43, 44, 46, 57-60.
  S14d (`exec`): 55.125, pf.42.
- **`FinalStage`.** S15p (the peel): 24.22-25, 24.28, 24.29, bng.17-19,
  pf.47-51, 61-66. S15s and S15b (suppressed): 24.29, pf.61, 63, 64. S15n
  and S15c (negated): 24.24, 24.25, 24.28, pf.50, 51. S15d (the terminal
  `!`): bng.17-19, pf.65, 66. S15e: none; a unit test pins it (below).
- **fd tags and the header `{`.** A1: csfx.12-18. A2: csfx.19. B1: 71.6-8.
  B2 (`{` ends a declaration outside a header too): 71.9.
- **Double fold.** DF: 55.137-142.
- **`return N`.** R4a (the old digits-only test): 74.67, 74.68. R4b (sign
  stripped, no i64 bound): 74.68. R4c (any signed word is a value): 74.69.
- **Arithmetic comma.** AC1: csfx.9, 11, acm.1, 3-10. AC2: acm.11, 12. AC3:
  55.123, acm.8, 10. AC4: 55.123, csfx.9, acm.1-4, 8-10. AC5: csfx.11,
  acm.5-7.
- **Stray keywords and clause positions.** K1 (`stray_block_keyword` off at
  a statement head): skw.1-12, 15-17, 20-25. K2 (the one-ahead check after
  `-`): skw.13, 14. K3 (`}`'s "closes nothing" message): skw.18, 19. K4
  (keywords judged after a parse error too): skw.29, where the leftover
  `then` and `fi` would repeat a header's error. K5 (`rediagnose_stray`
  off, so no SB4057 inside an unclosed `$(`): skw.30-33. K6
  (`cmd_sub_close_step` stops counting a nested `$(`): skw.32. K7 (its
  `$((`/`((` arm dropped): skw.33. CL1 (`elif`'s "expected 'then'" at the
  statement's position, not the `elif`'s): skw.26. CL2 (`parse_brace_elif`
  given the statement's position): skw.27. CL3 (`parse_brace_else`, the
  same): skw.28. PF67 (SB4073 hard-codes "while"): pf.67.

S1, S6b, S7, S7d, S8 and S10 also turn existing specs red, as listed. Zero
red: S22, S15e and base0.

**S15e** stubs `FinalStage`'s reader-loop test so that a negated final
`while` counts as a reader loop. No program reaches the difference, because
the stage gate already rejects `!` on a `while` final stage (SB4022). The
unit test `a_negated_final_loop_stage_is_not_a_reader_loop` pins it.

**pipeown.28 under S9b** is a harness artifact. S9b changes a declaration's
label, and pipeown.28 has no declaration. It failed once, in C mode, and
never again: 1 in 266 runs of pipe_functions.sh across both stub runs, and
none in 25 sweeps per mode. The spec injects a `clone` failure with
strace's `when=3`, aimed at the third fork (printf, sh, cat). When a child
exits during a clone's entry stop, the kernel restarts that clone
(ERESTARTNOINTR), and strace counts the restarted entry, so the fault can
land on sh's fork instead. The log shows exactly that: `/bin/cat finished`
found no file, because sh never ran. Three proofs show the mechanism, the
last with a real child exit in both backends. Of the suite's 39 count-based
injection rules, on 38 spec lines, pipeown.28's is the only one exposed.
`bug-strace-count-injection-drifts-on-restart` (P3) holds the mechanism,
the proofs and the options, and AGENTS.md now warns about it beside the
fault/resource specs.

## Gates

- `mk test`: 234/27/20, with eight new unit tests:
  - `background_fault_rejects_a_return_producer` and
    `pipeline_part_rejects_a_compound`: ICEs where an earlier check already
    refuses the shape;
  - `suppression_rejects_a_negated_body` and
    `suppression_rejects_a_negated_background_body`:
    `push_suppression_into_pipeline`'s negated arm, now an ICE, since `- !`
    is SB4019 before it is reached;
  - `an_elif_clause_records_its_keyword_position` and
    `a_flattened_elif_keeps_its_keyword_position`: the clause spans;
  - `a_fork_loses_what_an_internal_opcode_does`: every internal opcode
    acts on its process;
  - `a_negated_final_loop_stage_is_not_a_reader_loop`: S15e above.
- Final sweep, rows 6401061-6411069: 10009 = 9996 passing + 13 skips (native
  5114; c 4882 + 13 skips). Exactly the 108 predicted first-ever ids
  (74.67-69, acm.1-12, cnd.20-24 and skw.1-34, in both modes), no flips, no
  retirements; the can-fail query reports 260.
- Re-sweep, rows 6411070-6421078 (12:52-12:56 UTC): the same totals, no
  first-ever ids, no flips, no retirements. The source had been edited around
  the final sweep (12:29-12:33): `expr.rs` a minute before it and
  `stmt_handlers.rs` (the S15e unit) after it, so its log alone could not
  show which binary it ran. The binary (12:41:26) postdates every source
  edit, and the re-sweep ran it.
- The SB census is empty both ways; both collision checks are empty; `plan`
  matches `chk` but for `coverage.sh`'s documented +2; the `plan` sums,
  native 5114 + c 4895, equal the summary's 10009.
- The pre-commit hook passes with no bypass. The means are cc 3.47 and
  cognitive 1.50, so the gate's 3× bar is 10.4 and 4.5:

| Function | cc | cog |
|---|---|---|
| `background_fault` | 12 | 1 |
| `acts_on_its_process` | 11 | 1 |
| `condition_shape_fault` | 9 | 2 |
| `unsuppressible_error` | 7 | 1 |
| `cmd_sub_close_step` | 7 | 1 |
| `parse_suppressed` | 5 | 4 |
| `stage_body`, `stage_marks` | 5 | 3 |
| `dispatch_in` | 5 | 1 |
| `skip_to_cmd_sub_close` | 3 | 3 |
| `reject_statement_form_pipeline_stage`, `taint_stmt_no_read_taint` | 3 | 3 |

  The two above cc 10.4 are flat dispatchers (cognitive ≤ 2), which the gate
  accepts as-is.

## Ledger and findings

23 hunk-ledger rows are ported (18 F4, 5 F1). Four F1 rows are dropped:
H064, H065, H191 and H197, the branch's partial fix for B48.
`bug-peel-family-classifiers` cites them as reference. B49's two rows (H063,
and H213's F1 half) move to F5, whose B53 rewrite carries them. H262's F1
half is re-implemented; its W2 half stays pending.

The findings index ticks B11, B12, B19, B23-B26, B28, B31, B112, n1-n6 and
the double fold. B27, B42-B48 and B50 point at `bug-peel-family-classifiers`.
Three tasks close: `bug-dispatch-in-boolean-positions`,
`diag-elif-clause-has-no-span` and `q-empty-keyword-condition`.

## Decide-and-report

DG may veto any of these; each is marked in its DD addendum or task.
- The scope reading above: fix inside touched functions, file the rest.
- DD#55: control words count as reserved words under `&`, and the rest of
  the `&` class (assignments, declarations, `(( = ))`, `cd`, a bare `exec`,
  the in-process builtins).
- DD#122: the stage class beyond the control words B12 named; `a | -
  readline x` is legal and its verdict does not join; a redirect on the
  final reader is SB4022; a `while` final stage may carry `-` but not `!`.
- DD#70: an unconsumed terminal `!` is SB4018.
- DD#71: outside a header, `{` does not end a declaration
  (`q-statement-form-residuals`).
- DD#66: a return producer is a boolean position, and the diagnostic points
  at the dispatch's `[[`.
- DD#68: `q-empty-keyword-condition`'s option (a) (reject; DG may choose
  (b)); only a user-braced group counts; stray keywords in every command
  position, and `fn in` plus a call; a block that runs into a closer is
  reported at the closer; no keyword judging after a parse error; SB4057
  inside an unclosed `$(`.
- DD#31: a comma with a write is an assignment, not a compound, with the
  `&&`/`||` hold.
- DD#74: `return -1` is a value.
- `diag-elif-clause-has-no-span`'s three deviations, recorded in the task.
- The spec trim rule, and the kept ids of the three rewritten specs.
- The complexity refactors, and the S15e backstop unit test.
- pipeown.28's red under S9b is a harness artifact, filed as a P3 task
  rather than fixed here, with a new AGENTS.md note on strace's `when=N`.

## Backlog

26 new tasks:
- **P1:** `bug-peel-family-classifiers`, `bug-noop-drops-argument-effects`
  (`: ${s:?msg}` never aborts), `bug-return-status-string-operand`
  (`return $s` silently returns 0).
- **P2:** `bug-fork-failure-reporting-by-form`,
  `bug-function-name-shadows-command-path`, `bug-handling-branch-drops-latch`,
  `bug-sb5012-sb5094-nested-body-flow`,
  `bug-validator-taints-clean-pipe-while-reader`, `q-for-without-in-list`.
- **P3:** `bug-arith-comma-classified-compound`,
  `bug-arith-comma-nonfinal-part-unchecked`,
  `bug-array-literal-elements-not-operand-words`,
  `bug-declaration-names-unvalidated`, `bug-negated-reader-loop-false-sb4073`,
  `bug-sb4025-advice-misdescribes-rhs`, `q-arith-test-statement-status`,
  `q-clause-on-next-line`, `q-compound-as-condition`,
  `q-suppression-and-stage-residuals`,
  `bug-strace-count-injection-drifts-on-restart` (the harness's; see the
  gates).
- **P4:** `bug-c-int64-min-literal`, `bug-sb5012-message-names-no-command`,
  `dedupe-external-head-predicates`, `doc-keyword-lexing-claims`,
  `q-sb4072-closer-diagnostics`, `q-statement-form-residuals`.

New rows go into `bug-digit-led-names`, `bug-expr-type-int-disposition` and
`bug-parse-error-in-block-cascades-sb4072` (`fn f { - ! x; }` and `- str`
cascade there too).

## Corrected along the way

- An earlier tally counted 62 stubs. S4d was retired when the arithmetic
  comma change deleted the arm it stubbed; AC3 stubs its replacement.
- I wrote that each R4 stub pins exactly its own spec. R4a turns 74.67 and
  74.68 red.
- The stub table's "pre-existing" label was stale for specs written after
  its baseline (csfx.12-19, 71.6-9, 55.137-142, acm.*). They are new.
- status.md first said 233 unit tests; it is 234.
- A plan note said the header `{` had not been added to declarations. It
  had, header-only; the DD#71 addendum and `q-statement-form-residuals` say
  so.
- `q-empty-keyword-condition`'s table read an empty condition as false. It
  tested the previous command's status; the task's resolution says so.

## Correction, same day — `exec cmd &` is not `cmd &`

DG asked me to explain `exec cmd &`. The DD#55 addendum, this journal, the
`command_background_fault` comment, 55.128's comment and
`q-suppression-and-stage-residuals` Q2 all said it equals `cmd &`. It does
not, and I had not measured it. In both backends, `cmd &` and `- cmd &` fork a
sash child, and that child forks the command, so `$!` names the sash child. A
signal sent to `$!` stops that child and leaves the command running, orphaned.
`exec cmd &` forks once and the child becomes the command, so `$!` names it
and a signal reaches it. Like a foreground `exec`, it leaves nothing to report
a failure: `exec /bin/false &` prints nothing, as `- /bin/false &` does, and
the status still reaches `$(waitstatus)`. A pipeline stage's child already
execs its command, so there `exec` changes nothing, which is why SB4022 is
right for `a | exec b`. All five texts are corrected. Q2 now recommends
keeping `exec cmd &` (the way to get a job a signal can reach), and asks
whether a plain job's orphaning is acceptable. Only comments and notes changed.
Probes: `files/port-exec-probes.tar.gz`.

## Next

B37, when DG says go: resolve.rs's private walk skips an index, `$(( ))` and
`(( ))`, so a capture there keeps a bare head that the taint pass reads as
clean.
