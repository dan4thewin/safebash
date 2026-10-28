# 539 - A child runs only its own code

2026-10-28. F2 is Order item 10 of the batch-531 port. DG said "go for F2"
on 2026-10-27. Two discussion rounds followed, on B82 and then on `-` in
front of a user-function call. On 2026-10-28 DG ruled:
- `-` is for externals and system calls;
- nothing may cement `-` on user functions;
- every potential suppression change goes to the backlog
  (`q-suppression-on-user-functions`).

F2 lands as three commits:
- 7a3ca42 ports the branch's F2 without the B82 wrap, adding B81 and B120;
- 33616b6 makes `:` evaluate its arguments, a P1 folded in;
- this notes commit.

## The bug

A forked child is a pipeline stage or a background job. Each of these ran
code that belongs to the parent:

- **B9.** Every forked child ran END on its way out.
  - Cause: `install_end_block` rewrote each non-function `Return` into a jump
    to END, a stage's and a job's included.
  - Effect: a job forked inside END re-entered END recursively, a fork bomb.
- **B10.** A failing function stage was reported under the compiler's
  `__compound_N` label instead of the function's name.
- **B17.** A background job inside a reader loop panicked with "reader
  scopes must finish in nesting order".
  - Cause: the parent's reader cleanup followed the fork edge into the child.
- **B20.** A background job whose fork failed was silent.
  - Cause: `sash_fork`'s -1 was taken for the parent.
  - Effect: the job never ran, `$!` read -1 and the status stayed 0.
- **B21.** A self-tail function used as a stage drew a false SB5037.

Two more surfaced in the same code:

- **B81**, folded in from F6. A capture in a command stage's arguments is a
  compile-time ICE on hand-parser.
- **B120**, new and pre-existing. A function stage set up its call through a
  private copy of the inliner's argument handling.
  - The copy took the caller's positional array before it lowered the
    stage's arguments.
  - An argument that read the caller's `$1` was an ICE in both backends.
  - `$#` gave a wrong number in C.

## The fix

The branch's design is ported as-is. All 63 F2 rows of the hunk ledger are
ported, except `process_blocks` inside H158, which belongs to W4.

- **`Terminator::ChildExit`.** A forked child leaves through it with its own
  `$?`. It never jumps into parent code, END included.
- **`SsaProgram::child_region`.** The set of blocks a child runs. Native
  codegen and `emit_c` keep these blocks out of END's abort routing.
- **`enter_forked_child` / `exit_forked_child`.** At a child's entry they
  clear the parent's return target, loop stack, tail loop and pipe reader.
  At its exit they restore those, and the parent resumes with its own
  variable versions, none of the child's.
- **`compound_stages`.** Records which function a stage calls, so a failing
  stage is reported by that function's name.
- **B20.** A failed fork takes its own edge.
  - It reports `fork: error N`, the text a pipeline already uses when its
    construction fails (DD55).
  - The status becomes 1, and the job aborts unless it is checked or
    suppressed.
- **B21 and B120.** `build_function_pipeline_stage` now calls
  `inline_function_call` inside the child.
  - The arguments are evaluated in the caller's frame.
  - The callee's self-tail call becomes a loop there.
  - The private copy is gone.
- **B81.** A command stage's argument captures are lowered inside the stage
  child, after the stage's redirects. A `:` stage therefore runs its
  captures, as the statement `:` does.

**What is not ported.** The B82 wrap threaded a stage's `-` into a function
stage's child. DG's ruling drops it, together with the specs that pinned it:
- stgarg.10 and stgarg.21;
- 56.90;
- pf.72-81 and pf.84.

Their ids stay unused.

Two draft specs used `-` on a user function only incidentally. They keep
their subject and lose the `-`:
- stgarg.7, a stage inside a background job, now runs `f "$1" | /bin/cat`;
- bgfork.8, `$!` after a failed fork, now suppresses `- /bin/echo in &`,
  not `- g &`.

**Commit B: `:` evaluates its arguments** (`bug-noop-drops-argument-effects`,
P1, pre-existing in both backends). The bug: `: ${s:?empty}` printed the
next line instead of aborting. DD77, DD69 §2 and DD86 (c) all say `:`
discards its arguments' *values*, not their evaluation.

The fix is one effects-only helper per backend:
- native: `evaluate_args_for_effects`, called by `emit_noop` and by the `:`
  arm of `lower_pipeline_builtin`;
- C: `emit_args_for_effects`, in the Noop arms of `emit_command_body` and
  `emit_pipeline_stage`.

Every position is covered: statement, condition, redirected, stage, and the
plain form inside a stage child. The converter needs no change, because bash
already evaluates `:`'s arguments (57.192).

## Specs and proof

**Commit A: 56 new ids, 55 of them in C.**
- pipe_functions: stgarg.1-9, 11-20 and 22-36; pf.69-71 and 82-83;
  pipeown.42-43.
- signals: 56.89 and 56.91-93.
- functions: stgcyc.2-3.
- commands: bgfork.1-8.
- trace: tr.4 (native only).

The background fork-failure specs inject EAGAIN into `clone` with
`run_sash_under … strace`.

Negative control: on the pre-port binary, 51 of the 56 native ids and 51 of
the 55 C ids fail. The rest:
- stgarg.34 and stgarg.36 pin behaviour the port leaves unchanged;
- bgfork.8 and tr.4 guard against a branch regression;
- bgfork.6 needed a stub. Forcing `checked` in both backends'
  `emit_store_bg_pid` turns bgfork.6-8 red in both modes.

**Commit B: noop.1-13 (commands.sh) and 57.192 (convert.sh).**
noop.1-9 and 57.192 fail on the pre-fix binary. The four controls,
noop.10-13, pass on both.

**Sweeps.**

| Commit | Rows | Total | First-ever |
|---|---|---|---|
| A | 6431128-6441287 | 10160 = 10147 pass + 13 skip | 111 |
| B | 6441288-6451474 | 10187 = 10174 pass + 13 skip | 27 |

Both totals were predicted. Both sweeps had 0 flips and 0 retirements, and
can-fail returned 260.

**Other gates.**
- `mk test` is green (237/27/20).
- The pre-commit hook passed on both commits, with no bypass.
- Re-run today on HEAD:
  - no duplicate `chk` id and no variable-id collision;
  - plan equals chk in every file except coverage.sh's known +2;
  - the SB census is empty in both directions.

## Decide-and-report (DG may veto)

1. **B81 is folded in from F6.** Its fix lowers the captures in F2's stage
   child, so it is F2's code.
2. **`:` builds no argv, so it runs no DD99 NUL guard.** A NUL-bearing
   argument never crosses execve(2) and stays a legitimate value, as in an.5
   (noop.13; DD69 addendum 2026-10-28).
3. **A failed background fork uses the pipeline's text**, `fork: error N`
   (y17).
4. **H243 is re-pinned.** The branch's stage rewrite deleted the unit test
   `recursion_into_an_inlining_function_unwinds_what_it_took`. Its claim now
   lives in `recursion_is_refused_before_the_call_takes_any_caller_state`,
   on `inline_function_call`'s SB5037 backstop.
5. **Stage and statement order stays as it is.** A stage applies its
   redirects before it evaluates its argument captures; a statement does
   the reverse. This is filed as a question
   (`q-stage-capture-redirect-order`); stgarg.33-36 pin the current order.
6. **SB5037 reports a mutual cycle once or twice** depending on definition
   order (bf24). Filed in the backlog at P4.
7. **The by-form fork-failure task stays in the backlog.** It gains row e1:
   a fork failure inside END is still silent. Its "lands with B20" lines are
   corrected in commit A.

## Corrections

- **DD55.** The F1+F4 addendum said a background fork failure is silent
  (B20, open). It now says such a failure aborts as written since F2. END is
  the exception.
- **Journal 532.** The stage bullet blamed an ICE on the missing tail-loop
  take. The ICE was B120's: the argument `$(( $1 - 1 ))` read the caller's
  `$1`.
  - Without the take, a stage whose argument does not read `$1` silently
    runs the recursion DD27 forbids: stgcyc1v prints `g0|g1|g2`, rc 0, in
    both backends.
  - Journal 532 now carries a pointer to this correction. The findings
    entry for B21 is corrected the same way.
  - `spec-harness-hides-compiler-ice` gains a dated note: its reproducer no
    longer ICEs, and the harness gap it describes is unchanged.
- **`sweep-private-duplicates-of-shared-walkers`** records B120 as its fifth
  instance.

## Measured, not changed

- **q1h.** Since commit B, `- : ${s:?empty}` aborts: `-` does not discard a
  disposition's abort. Recorded in the Q1 table of
  `q-suppression-and-stage-residuals`, which waits for DG's suppression
  rulings.
- **B9's open question.** A child forked inside END is unchecked (q1, q2),
  as the DD56 addendum records. Filed as R4 of `q-f2-residuals`.
- **t4.** A `while` body does not reset `$?`. Filed as a section of
  `bug-handling-branch-drops-latch`.
- **b17d.** SB5094 fires at 4:9 and 6:9 in every version, while its twin is
  legal. Filed as a row of `bug-sb5012-sb5094-nested-body-flow`.

## Backlog

New tasks:
- `q-f2-residuals` (P3) covers four questions:
  - R1: `wait` with a pid <= 0 reaps whichever job finishes first;
  - R2: echo children interleave mid-line;
  - R3: a bare `${s:?}` aborts with an empty message;
  - R4: children forked inside END are unchecked.
- `q-stage-capture-redirect-order` (P3).
- `bug-sb5037-cycle-reported-twice` (P4).
- `bug-write-diagnostics-name-echo` (P4): a `write` failure is reported as
  `echo:`, because `rewrite_write_as_echo` turns the command into an echo.

Rows added to existing tasks are listed under "Measured, not changed"
above. The probes are in `files/port-f2-probes.tar.gz`.

**Ledger and index.**
- 63 F2 rows are marked ported. H158 is ported except `process_blocks`
  (W4). H243 is re-pinned.
- The findings index ticks B9, B10, B17, B20, B21 and B81, and adds B120.

## Next

The port has completed 10 of its 20 Order items. The next is F3 (B8/B16,
END-defined functions), which waits for DG's go.

Rulings still pending:
- the seven F1+F4 question tasks;
- B37's P1, (a) SB4049 or (b) lift;
- the SB4049 advice wording;
- the suppression question tasks;
- the offered `mk gate`.
