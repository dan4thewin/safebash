title: DD#55 — Implicit Exit Checks and Error Handling Model
status: implemented

No `set -e` at runtime. The compiler inserts implicit exit checks on
every operation that can fail at runtime. Failure = nonzero exit or
runtime error (e.g., `regcomp` on dynamic pattern). `- cmd` suppresses.

## What gets checked

Anything that can fail due to external state or data:

- External commands (fork/exec → wait → check status)
- Builtins that wrap syscalls (`cd`, `echo`/`printf` via `write(2)`)
- Runtime calls that can fail on data (`regcomp` on dynamic patterns)
- Command substitution (`x=$(cmd)` — check before assignment)

## What does not get checked

- Variable assignment (`x="hello"`)
- Arithmetic evaluation (`(( x = x + 1 ))`)
- Pure string operations
- Literal regex (validated at compile time, can't fail at runtime)

## Suppression with `-`

`- cmd` suppresses the implicit check. The command runs, its exit
status is discarded. `$?` is not available after `- cmd` — reading
`$?` after `-` is a compile error. You said you don't care; you
don't get to inspect it.

## Explicit handling

Three patterns for a command that can fail:

| Pattern | Meaning |
|---------|---------|
| `cmd` | Implicit check — abort on nonzero |
| `- cmd` | Suppressed — don't care, `$?` unavailable |
| `cmd \|\| action` / `if cmd` / `while cmd` | Explicit handling — `$?` available in handling position |

`$?` is scoped to the handling expression (`||` RHS, `if`/`while`
body), not available as a general-purpose variable in subsequent
statements. Furthermore, `$?` (or `$(status)`) is only valid as the
**first expression evaluated** in the handling branch — once another
command executes, the status is consumed and unavailable. This
prevents stale-status bugs where `$?` refers to something other
than the command that caused entry to the branch.

## `local` declarations

`local x=$(cmd)` — the capture's exit check fires regardless of
`local`. sash does not swallow exit codes in declaration context.
This is an explicit departure from bash, where `local` is the
command and its success masks the substitution's failure.

## Command substitution

`x=$(cmd)` — implicit check fires on cmd's exit status. If cmd
fails, the program aborts. `x` is never assigned.

`x=$(- cmd)` — suppressed. cmd runs, output captured, exit status
discarded. `x` gets whatever output cmd produced before it exited.

`x=$(cmd) || x=default` — explicit handling. If cmd fails, the
`||` branch executes.

### Suppression scopes the complete call (2026-10-20 correction)

The prefix includes argument evaluation and the called body, including
commands in branched bodies. It must neither miss blocks allocated after an
inlined return block nor uncheck blocks belonging to an earlier operation.

Suppression does not change the produced value or its type: a captured
`-> int` function still yields its integer return (DD#58), and a string
function still yields stdout. Sole value intrinsics retain their value
position. Structural arity checks, taint and status availability remain in
force; this is not a validation bypass.

For `return $(- self ...)`, suppression starts at that tail call, including
its arguments. Earlier work in the initial invocation stays checked; later
iterations inherit suppression while retaining constant-stack tail lowering.

(**Under question, 2026-10-28:** DG doubts that `-` on a user-function call
should reach into the function at all; see
`notes/tasks/q-suppression-on-user-functions.md` Q1 and Q4. Unruled, so this
text stands until DG rules.)

## Pipelines

Pipefail always on (DD#20). Implicit check fires on the pipeline's
aggregate status. Per-segment suppression with `-`:

```
- cmd1 | cmd2       # cmd1 failure suppressed, cmd2 checked
cmd1 | - cmd2       # cmd1 checked, cmd2 failure suppressed
- cmd1 | - cmd2     # both suppressed
```

If any unsuppressed segment fails, the program aborts. SIGPIPE is
the programmer's responsibility — use `- cmd` on the producing
side or choose tools that don't trigger it (`sed -n 1p` over
`head -1`).

Per-segment `!` negation is the companion operator (DD#70 addendum
2026-08-31): `cmd1 | ! cmd2` inverts the **terminal** segment's status
(0↔nonzero) before it joins the worst-of below. Only the terminal segment
may be negated (a non-terminal `!` is SB4040); `-` and `!` don't combine on
one segment (SB4019).

### Aggregate is context-independent; position decides the consequence (2026-07-24)

The pipeline's truth value is **one** definition everywhere: the worst
exit status over its **non-suppressed** segments (pipefail). *Position*
decides only the consequence, never the aggregate:

- **statement position** — a nonzero aggregate aborts (routes through END).
- **condition position** (`if` / `while` / `until` test, operand of `!`) —
  the aggregate is the truth value; it does not abort.

So `if /bin/false | /bin/true; then …` takes the **else** branch — a
first-segment failure counts, exactly like bash `set -o pipefail`. A
`-` on **any** segment (first, middle, or last) excludes only that
segment's status from the aggregate; the segment still **runs** (it is a
real pipe stage — `echo x | - grep y` runs grep, it is not dropped).

This corrects a prior implementation bug where a pipeline in a non-abort
context silently ignored its **first** segment (an undocumented
`i == 0 && !checked` skip that conflated leading-`-` suppression with
condition context), and where a non-first `-` dropped the stage entirely.

### Parent-reader pipelines stream before checking (2026-10-18, DG)

DD#122's parent reader consumes concurrently with its producers. Only after
the consumer completes or leaves its scope does it close the read end, reap
the producers and evaluate their aggregate. Waiting before consuming can
deadlock on pipe capacity; it is not required by the error model.

Consumer effects before a producer's eventual failure remain visible. Early
completion may cause producer SIGPIPE, handled by the same suppression and
pipefail rules above. Successful cleanup preserves a nonlocal return or loop
transfer's existing status; checked producer failure still follows the normal
abort path. Abort/explicit exit closes all outstanding readers before reaping
and retains its triggering status for END. No forced kills or termination
timeouts.

An authored `$(wait)` can reap a pending producer; the owning reader retains
that status for its aggregate instead of attempting a second wait and reporting
a spurious 127. The public wait pid/status result is unchanged.

### Pipeline construction failure (2026-10-17, DG)

If pipe creation or a later fork fails, stop constructing the pipeline,
close its pipes, and wait for the stages already started. Do not send
SIGKILL or otherwise force their termination: as DG put it, "we can't know
the harm a SIGKILL could cause - close and wait". Waiting may take as long
as those stages need; that is deliberate, not a reason to add a kill timeout.
The construction failure still follows the normal caught/uncaught error
handling above.

Both backends check allocation/fork results before consuming pipe outputs or
recording a pid. A failed capturing pipeline skips its reader/loop entirely;
construction failure is not EOF and must not clear or overwrite the consumer's
target. Child fd wiring or redirect failure prevents that stage's body from
running.

A single external `$(command)` uses the same checked pipeline construction.
Uncaught failure enters END; caught construction failure binds empty output
and a false condition. `$(- command)` discards failure without aborting, but
failed setup never executes the command. The capture drains its private pipe
before waiting and does not replace an enclosing lastpipe reader.

### `&&` / `||` / `!` do not take a pipeline operand

A pipeline may be an **entire** condition (`if a | b; then …`, `while a | b`),
but it may **never** be combined inline with `&&`, `||`, or `!`:

- `a | b && c` / `a | b || c` — **SB4023**.
- `! a | b` — **SB4040** (DG 2026-08-19).

The reason is **reviewability, not semantics**. bash's precedence resolves all
of these unambiguously (`|` binds tighter than `!` and `&&`/`||`), so the exit
status is well-defined. But **sash is meant to be easier to review than bash**:
a non-expert reviewer shouldn't have to know operator-vs-pipe precedence to see
what `&&` tests or how far `!` reaches. So sash rejects the inline forms and
requires the explicit shape:

```
if a | b; then c; fi                  # instead of  a | b && c
if a | b; then :; else c; fi          # instead of  a | b || c
f() { if a | b; then return 0; fi; return 1 }   # instead of  ! a | b
if ! f; then …                        #   (wrap the pipeline in a predicate fn)
```

A pipeline as the **consequent** (`cmd && a | b`), a trailing redirect
(`a | b > f`), and a background pipeline (`a | b &`) remain legal — only the
inline `pipeline &&/||/!` combinations are rejected. `!` on a single unit (one
command, `[[ ]]`, `(( ))`) is fine, and binds only that unit — `! X && Y` is
`(!X) && Y` (DD#70).

## Background processes (`cmd &`)

Exec failure (fork or exec fails) is checked — the parent aborts.
(**Corrected for exec**, and the `&` forms narrowed — addendum 2026-10-26.)
A successfully forked background process runs independently and
can outlive the sash program. `wait` is optional.

**Amended by DD#126 (2026-10-09).** `wait` no longer returns a status and is
no longer subject to the implicit check: it is an int intrinsic yielding the
reaped **pid**, or `-1` on `ECHILD`, and it never aborts. The reaped child's
exit is read through `$(waitstatus)`. A bare `wait` statement is SB4016 — the
unconsumed-value rule, not the implicit-check rule.

## User-defined functions

### Void functions

No `return` statement, or unreachable (all paths abort or loop).
Bare call at call site is fine — no check needed.

### Non-void functions

Any `return` statement makes the function non-void. Call sites must
explicitly handle the return value:

- `if myfunc` / `myfunc || handle` — explicit handling
- `- myfunc` — suppressed
- bare `myfunc` — **compile error** (unhandled non-void return)

(**Under question, 2026-10-28:** whether `- myfunc` stays legal, and how a
bare call of a status-returning function is handled (B15), are open in
`notes/tasks/q-suppression-on-user-functions.md` Q2 and Q3. Unruled.)

`return 0` alone is a compile error — if the only possible return
is zero, the `return` is ceremony. Omit it (void function).

### Bare call of int-typed function

Compile error. If the function returns a value, capture it or
handle it. A bare call discards the return value.

### `return $?`

Valid only in handling position: `cmd || return $?`. Propagates
the failure to the caller, making the function non-void, which
requires explicit handling at every call site.

`- cmd; return $?` is a compile error — `$?` is unavailable
after `-`.

## Comparison with bash `set -e`

Bash `set -e` has known blind spots:

- Functions called from `if` lose errexit for their entire body
- `local x=$(cmd)` swallows exit codes
- `(( 0 ))` kills the script
- Subshells inherit errexit inconsistently

sash has none of these. Checks are inserted per-operation by the
compiler. Condition position is recognized syntactically. `local`
doesn't swallow. Arithmetic doesn't produce exit codes.

## Compound-condition catch + list/body abort (2026-08-19, DG)

Externalizing `true`/`false` unmasked a pre-existing bug and two design
questions; DG resolved them:

- **`&&`/`||` operands in a condition are CAUGHT.** `if a && b` / `while
  a || b` treat the whole `&&`/`||` aggregate as the condition — every
  operand's failure is caught, never aborted. Previously only the operand
  whose block terminated the condition's `Branch` was unchecked (the left of
  each `&&`/`||`, via the short-circuit block); the **right** operand sat in a
  `Goto`-terminated block and aborted. Fix: in `transform/statements.rs::
  lower_list_stmt`, when the List is in a *caught* position (`in_condition >
  0` — a condition, or the left of an enclosing `&&`/`||`), uncheck the right
  operand's terminal command too.

- **`if a; b` is `a; if b`** (DG). A `;` list's non-final member is a **bare
  command** whose status is discarded → it **aborts** (bare-command-uncaught).
  Only `&&`/`||` operands (logical parts of one boolean) are caught; a `;`
  sequence is not. So `if a; b` runs `a` (aborts on failure), then `if b`.

- **A failing FINAL command in an if/else BODY aborts** (DG). Its status is
  discarded, exactly like a top-level `false` — bare-command-uncaught. The old
  `bc.e1`/`bc.e2` rc-0 expectation was a non-aborting-builtin artifact.

Coupled fix: the `||` arm reset `$?=0` after the right side ("the list
succeeded") — correct for a top-level guard (`a || fallback`), but wrong in a
condition, where `if false || false` must take the else. The reset is now
skipped in a caught position (the aggregate's real status is the condition
value). native+c; janet emits conditions from the AST (pre-existing
control-flow gap, `cf-backend-gaps-brace-surfaced` — brace.sh is native-only).
spec brace.sh bc.a6b/a7/a8/a9/e1/e2, errors.sh cc.1-4. Journal 153.

## Addendum (2026-10-26): the condition is the last status-setting instruction

A condition catches the status of its operand and nothing else. Three scans
decided which instruction that was: the transform's, which clears the
operand's abort, and one in each backend, which marks the operand's redirect
open(2) failure as caught (DD#70). Each listed the instructions it recognised
and passed over the rest. A `[[ … ]]`, a `readline` or `read`, a `match`, an
`attest … in`, the dispatch form or a `return [[ … ]]` in condition position
was therefore skipped, and the scan settled on whatever command came before
it. That command's failure, and its redirect failure, went unchecked.

The operand is now the **last instruction in the condition's block that sets
the status** the block's `Branch` or `Loop` reads. `ssa::condition_operand_index`
is the one definition. The transform clears the abort of that instruction
only, and both backends treat that instruction's redirect alone as caught
(`ssa::condition_inst_index`). Its classifier, `ssa::sets_condition_status`,
is total over `SsaInst`.

Behaviour that changes, all of it previously silent:

- A command or a redirect failure **before** such a condition aborts, as it
  would before any other statement. That includes the non-final member of
  `if a; [[ … ]]`, which this DD already treats as `a; if [[ … ]]`.
- A failed capture **inside** a `[[ … ]]` operand aborts: the capture is an
  argument of the test, not the condition. A function capture there already
  aborted. An external capture can only reach a test operand under
  `--no-taint`, where `while [[ $(cmd) … ]]` used to loop on `cmd`'s failure.
- `cmd > /bad; if [[ … ]]` no longer "catches" `cmd`'s failed open.

Unchanged, and pinned now because no other spec reaches those arms of the
classifier: a pipeline as the condition, and a redirected function call.
The inlined call ends the caller's block in a `Branch` on the redirect,
whose operand is the redirect scope. Both already kept the abort of the
command before them.

A `$?` assignment (`return N`, DD#74) sets the status too. No program can
tell that arm from its opposite, because each producer ends its block in a
`Goto` or `Return`, or opens a fresh block in which nothing before the
assignment sets the status; a unit test in `ssa.rs` pins it instead. A reader's
`PipelineFinish` is also counted, but cannot reach the scan. Reader cleanup,
which runs before the scan and is its only producer, never leaves one in a
block that ends in a `Branch` or `Loop`: on such an edge it adds a bridging
`Goto` block (`notes/tasks/q-condition-classifier-pipelinefinish.md`).

Ported from the batch-531 reference branch (finding B85), as-is. Specs:
errors.sh 55.89-55.106. Journal 536.

## Addendum (2026-10-26, DG): what may run under `&`, and where `-` reaches

DG: *"I don't see reserved words being meaningful within a pipeline or in
combination with &."* A background job runs in a forked child, and it has no
status until it is reaped (DD#126). Both facts give SB4024, raised at parse
time, so it comes before anything a later pass would say about the statement.

**What a job may run.** A statement whose effect lands on the process that runs
it would act on the child and be lost.
- Compounds (DG's reserved words). `if … &` and `{ … } &` were already
  SB4024; they now are too when a `!` or a list hides them (`! { … } &`,
  `a && ! { … } &`).
- `return`, `break`, `continue`, `exit` (decide-and-report: DG's "reserved
  words" read as including the control words). `return N &` returned into the
  caller's code in the child, `break &`/`continue &` ran the rest of the loop
  and program in each child (batch-531 B11), and `exit N &` exited only the
  child.
- The rest of the class (decide-and-report): an assignment or typed
  declaration, an arithmetic write `(( n = 1 ))`, `cd`, an `exec` with no
  command, and `attest`, `argv`, `read`, `readline`, `keys`, `match`,
  `matchall`, `close`, `unset`, `signal`, `wait`. Most compiled and lost the
  effect with the child; the others drew an unrelated code (a declaration
  SB4014, B28; a bare `wait &` SB4016; `match`/`matchall` SB4018; `keys m ks &`
  a false SB5050 on `m`). Three miscompiled. After `attest x … &` the parent's
  read of `x` compiled as attested (C printed the unattested value, native an
  empty one). After `argv ar &` or `read ar &` the parent read a collection
  nothing had filled (batch-531 B13): the native binary segfaulted and the C
  one printed a garbage length. The classifier is
  `ssa::BuiltinCmd::acts_on_its_process`, shared with the stage gate (DD#122
  addendum 2026-10-26).
- Still legal: `cmd &`; `a | b &` (§Pipelines above; it drew a false SB4022,
  B23); `a && - b &`; and `exec cmd &`, which is not `cmd &`. (Corrected
  2026-10-27: this bullet first said `exec cmd &` replaced the child as
  `cmd &` would.) A job `cmd &` is a sash child that runs `cmd` as a second
  process, checks it and reports a failure. `$!` names that child, so a
  signal sent to `$!` stops the child and leaves `cmd` running. Under `exec`
  the child becomes `cmd`: `$!` names the command and a signal reaches it.
  As with a foreground `exec`, nothing is left to report a failure, so it
  reports as `- cmd &` does. Either way the status reaches `$(waitstatus)`
  through `wait` (DD#126). A pipeline stage's child already becomes its
  command, and the parent checks it, so `exec` there changes nothing and
  `a | exec b` is SB4022. Whether `exec cmd &` stays legal is open:
  `notes/tasks/q-suppression-and-stage-residuals.md`, Q2.

**Who may read its status.** A job as a condition's last statement, or as a
return producer, is SB4024: the consumer would act on a status the job never
produced. `if /bin/false & { … }` took the branch (B26), and
`return /bin/true &` made `if f` true (n4). A terminal `!` in a job inverts a
status nobody reads: `a | ! b &` is SB4018 (DD#70 addendum 2026-10-26).

**Where `-` reaches.** At a statement's head `-` takes the whole statement,
a trailing `&` included (DD#126). What follows the `&` is a new statement, as
it is without the `-`: `- a & && b`, `- a & || b`, `- a & | b`, `- a & >f` and
`- a & &` are SB4014, as `a & && b` is. The parser folded the statement a
second time and joined what followed the `&` to the job: three of them
compiled (`- a & && b` ran `b`, and `- a & &` ran the job inside a second
job), and `| b` and `>f` reached an ICE. As a list operand or a pipeline stage
`-` takes that operand alone. It used to take the rest of the statement, so
`a && - b || c` slipped past SB4053 (DD#95; B24), and `a && - b &`
backgrounded `b` alone: in `cd /tmp && - /bin/true &` the `cd` ran in the
parent. The job is now the whole list, as in `a && b &`, so that `cd` is
SB4024. The check looks through the `-` as well: `a && - cd /tmp &`, an
assignment or `(( n = 1 ))` behind `-`, and a suppressed final reader
(`a | - readline y &`, `a | - read ar &`) are SB4024. The first three compiled
and lost their effect with the child; the readers drew the compound-stage
SB4022. `-` prefixes a command: on `attest`, a declaration, `fn`/`function`
or `END` it is SB4014 at the `-`. These drew SB5011 "command '-' not found" or
an unrelated SB4014. `- until` now parses like `- while`.

**Correction to "Background processes" above.** "Exec failure … the parent
aborts" is wrong for exec. The exec fails in the child, which prints the errno
and "command failed (exit 127)" and exits 127. The parent sees the status only
by reaping it through DD#126's latch (55.16). Under `- cmd &` and `exec cmd &`
the child prints the errno but not the "command failed" line. Fork failure is
meant to abort as written; today it is silent (batch-531 B20, open).

Specs: errors.sh 55.107-55.144, 71.4, bng.18, bng.25, csfx.4-csfx.8;
pipe_functions.sh pf.52. Journal 537.
