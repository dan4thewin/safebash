title: DD#55 — Implicit Exit Checks and Error Handling Model
status: implemented

No `set -e` at runtime. The compiler inserts implicit exit checks for
operational failures: nonzero command status or a runtime error such as
`regcomp` on a dynamic pattern. `- cmd` discards that command's status;
it does not suppress every failure in its operands or body.

## Terminal handlers (DD#128)

An uncaught main-process failure enters **ERR**, not EXIT. Normal
fall-through, explicit `exit 0` and a configured EXIT signal enter **EXIT**.
Explicit nonzero `exit N` enters ERR, using the full i64 value before process
exit truncation (DD#89). Children run neither handler.

EXIT is ordinary checked code: its uncaught failure enters ERR. ERR must
catch or suppress every operational failure where it actually occurs;
otherwise SB5101 rejects the operation or call. This is not a blanket
unchecked region. The no-reach rule below applies unchanged to helpers,
operand captures and setup failures. If no existing form catches an
operational failure, that operation is unavailable in ERR.

Arithmetic faults and heap exhaustion remain fatal exceptions. A first
fault in main or EXIT attempts ERR once without allocating for dispatch;
a fault in ERR terminates. Neither handler resumes interrupted code or
re-enters itself, and ERR cannot transfer back to EXIT. Falling off a handler
preserves its entry status; explicit `exit N` overrides it. `$?` is available
as the first evaluated expression on entry under the usual handling rule.

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

### A `-` discards one status (2026-10-30; replaces the 2026-10-20 correction)

A `-` discards the status of the one command or call it prefixes, and
nothing else. It does not reach into a user function's body or an
anonymous `$( … )` body, and it does not reach a capture or an `-> int`
call among the operands. Each of those is checked where it runs, as it
would be without the `-`. DG ruled this on 2026-10-29
(`notes/tasks/q-suppression-on-user-functions.md` Q1 and Q4), and F6
landed it (addendum 2026-10-30, "a `-` does not reach").

Suppression does not change the produced value or its type: a captured
`-> int` function still yields its integer return (DD#58), and a string
function still yields stdout. Sole value intrinsics retain their value
position. Structural arity checks, taint and status availability remain in
force; this is not a validation bypass.

(The 2026-10-20 correction said the opposite: the prefix covered argument
evaluation and the called body, and `return $(- self …)` suppressed every
later iteration of the tail loop. Journal 530 built that. Journal 547
retired the tail-loop half (SB4076), and journal 548 the rest.)

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

- **statement position** — a nonzero aggregate aborts (routes to ERR).
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
and retains its triggering status for the selected handler. No forced kills
or termination timeouts.

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
Uncaught failure enters ERR; caught construction failure binds empty output
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
`notes/tasks/q-suppression-on-user-functions.md` Q2 and Q3. **Ruled
2026-10-29/30** (that task's two "Rulings" sections). `- myfunc` stays
legal on a function that returns a value, a status or an int, and is an
error on a void one. A bare call of a status function is an error unless it
is caught (`if`, `||`) or discarded with `-`, as above. A bare `return`
counts as `return 0`, and a tail call `return g` passes g's status through.
F6 landed the ruling on 2026-10-30 (addendum at the end). F6's unit B (the
last addendum) makes `- myfunc` discard only myfunc's own answer; a failure
inside its body still aborts. The ceremony rule
below stays `-> int`-only (SB4017) until `q-status-function-ceremony` is
ruled.)

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
or either handler declaration it is SB4014 at the `-`. These drew SB5011
"command '-' not found" or an unrelated SB4014. `- until` now parses like
`- while`.

**Correction to "Background processes" above.** "Exec failure … the parent
aborts" is wrong for exec. The exec fails in the child, which prints the errno
and "command failed (exit 127)" and exits 127. The parent sees the status only
by reaping it through DD#126's latch (55.16). Under `- cmd &` and `exec cmd &`
the child prints the errno but not the "command failed" line. Fork failure
aborts as written since F2 (2026-10-28, batch-531 B20, journal 539):
`fork: error N`, status 1, abort unless checked or suppressed, and `$!` reads
-1. DD#128 removes END's silent-handler exception: EXIT is checked, and
ERR requires an explicit handling form (`bug-fork-failure-reporting-by-form`,
historical row e1).

Specs: errors.sh 55.107-55.144, 71.4, bng.18, bng.25, csfx.4-csfx.8;
pipe_functions.sh pf.52. Journal 537.

## Addendum (2026-10-29, batch-531 port F5): a body called from a condition is checked

The set -e comparison counts "functions called from `if` lose errexit for
their entire body" among bash's blind spots, and says sash has none of
them. DD86 says the condition context stops at a body. The transform
disagreed: `in_condition` stayed raised while an inlined body was lowered.
So a function called as an `if` or `while` condition, under `!`, as the
left operand of `||`, as an `-> int` call in an arithmetic condition, or as
`$(h)` inside `[[ ]]` ran its whole body unchecked, and its failures were
swallowed (B83). An anonymous `$( … )` body in a condition command's
arguments leaked the same way (B83b).

Every function and capture body is now lowered outside the condition
context. An unhandled failure inside it aborts, as it would at top level.
The call's own status is still the condition's. When the body runs as a
forked pipeline stage (`if a | h`), the abort ends that child, with its own
report, and the condition consumes the child's failure.

Suppression is not part of this change. Whether `-` should apply to a user
function at all is `q-suppression-on-user-functions` (DG, 2026-10-28).

Specs `errors.sh` 55.76-55.87; journal 542.

## Addendum (2026-10-30, batch-531 port F6): a non-void function's answer is handled or discarded

This lands the rulings of 2026-10-29 and 2026-10-30 in
`q-suppression-on-user-functions` (Q3, and Q2 for in-process calls), and
batch-531 B15.

**Three classes.** A function returns an int (`-> int`), a status, or
nothing (void). It is void when its only exits are `return 0`, a bare
`return` (which returns 0), falling off the end, and self-tail calls. A tail
call `return g …` hands back g's result, so a function whose `return g`
exits all reach void functions is void, and one that reaches an int or a
status function returns a status (an int g's value becomes the status). Any
other return producer gives a status: an external command, `[[ ]]`,
`(( ))`, a status builtin, or a redirected call such as `return g >file`.
A call is checked against the definition it runs, which is the one in
force where the call stands. Each `fn` statement recomputes the classes in
force as it comes into force, so a name defined twice can be a status at one
call and void at a later one (nonvoid.25-nonvoid.29). The `fn_NAME`
template of each body, and SB4017's scan, have no call site. They use a table
built before any body is lowered, over every program-level definition
(`Program::function_defs`; SB5071 permits no others). That table holds only
the names defined once and the functions whose class depends only on those. A redefined name, and
anything whose class it decides, is checked where the call is inlined
(nonvoid.26). Before, only `-> int` functions were classified, in textual
order while lowering, so a body calling an int function defined after it went
unchecked (nonvoid.9).

**A status function's status is handled or discarded (B15).** A call is
handled as a condition (`if`, `while`, the left side of `&&` or `||`), or
its status is discarded with `-`. Anywhere else it is SB4016, "function 'f'
returns a status — handle it with `if` or `||`, or discard it with `-`": a
bare statement, the tail of a list (`/bin/true && f`), a background job
(`f &`), in a function body or in EXIT/ERR. Reading `$?` afterwards does not
handle it. A capture `$(f)` binds the output and leaves the status nowhere
to go, so it is SB4016 as well, with the advice `$(- f)` (`$(- f …)` when it
has arguments). A capture as a condition, `if x=$(f)`, stays SB4025 (DD#88),
and the SB4016 at the same position is dropped. A redirected capture body,
`$(s 2>&1)`, lowers as a statement and gets the bare-call text.

**`-` on a void function is SB4076** (new): "function 'f' returns nothing,
so `-` has nothing to discard — call it without `-`". A capture of an int
function binds the int, so `$(- f)` on one is SB4076 too: "`$(f)` binds the
int that function 'f' returns, so `-` has nothing to discard — capture it
without `-`". A stage or a job is a process with a status of its own, so
`- v | /bin/cat` and `- v &` stay legal for a void v (nonvoid.24).

**SB4017 sees return producers.** The ceremony rule stays `-> int`-only
until `q-status-function-ceremony` is ruled. Its scan skipped producers, so
an int function with `return 0` on one path and `return g` (an int g) on
another was rejected as "only returns 0" (nonvoid.20), and `return g` with a
void g compiled and returned 0 (nonvoid.16). The first now compiles; the
second is SB4017. A function whose only exits are self-tail calls is void
and draws no SB4017.

**Deleted.** The suppressed int self-tail specialization (journal 530)
lowered `return $(- f …)` inside an int f as a second, suppressed copy of
the loop. That spelling is now SB4076, so nothing reaches it. It is gone,
with `TailLoop.entry_readonly`.

Decide-and-report (DG may veto): the three classes as stated, `return g
>file` as a status, `f &` as SB4016, SB4025 superseding SB4016, SB4076 for
`$(- int_f)`, the advice texts, and the class in force at each call. Not
changed here: whether a `-` on a call reaches into the body, or into a
capture among the call's arguments (`q-suppression-on-user-functions` Q1
and Q4, the next F6 unit). The questions this unit raised are in
`notes/tasks/q-nonvoid-classifier-residuals.md`.

Specs: errors.sh nonvoid.1-nonvoid.29. Rewritten to the rule: errors.sh
rdr.39, supval.1-supval.6, supval.9-supval.12, supval.14-supval.17,
supval.22 and supval.28; functions.sh 74.55-74.60, 74.64b, 74.67, tmpcap.19
and tmpcap.20; coverage.sh cov.330; pipe_functions.sh pf.71; taint.sh
div.2; types.sh constw.19. Journal 547.

## Addendum (2026-10-30, batch-531 port F6 unit B): a `-` does not reach

DG ruled on 2026-10-29 that a `-` on a call discards only the function's
returned value (`q-suppression-on-user-functions` Q1 and Q4). Taken with
DD#86 (a body is a function body, checked once), the rule for every `-` is:
**a `-` discards the status of the one command or call it prefixes, and
nothing else.** These are checked as if the `-` were absent:

- the body of a called function, whether the call is a statement (`- f`),
  an `-> int` call, a job (`- f &`), a capture (`$(- f)`), a stage, or an
  int call inside `(( ))`;
- a capture among the command's operands, for an external, a builtin, a
  value intrinsic or a `[[ ]]` test, and the body of a function captured
  there (`- echo $(g)`);
- a capture or `-> int` call in the operands of a suppressed `(( ))` test.

So `fn f { /bin/false; echo x; return 1 }; - f` now aborts at the
`/bin/false`; it used to print `x` and carry on. `f`'s `return 1` is still
discarded (noreach.2). Under a capture condition a function stage that fails
makes the condition false (noreach.6, .13), because the child aborts and the
condition sees a failed stage.

**Mechanism.** The transform keeps a suppression depth on `VersionState`.
`lower_suppressed_stmt` raises it around the command it lowers, and every
site that emits a status check reads it as it emits. Lifting a command's
operands and arguments, lowering a `(( ))` test's operands, and inlining any
body run at depth zero (`functions::unsuppressed`, `lower_inlined_body`). A
suppressed capture `$(- cmd …)` passes a flag to its own command instead of
raising the depth, so the depth never reaches its operands. The old
`SuppressionScope` unchecked every block allocated during the suppressed
statement after the fact. That is what reached into bodies, and it is
deleted.

**Pins**, by reversion stub in both modes: removing the take in
`lower_inlined_body` turns noreach.1, .3, .4, .6 and .15 red; removing the
`(( ))` operand barrier turns noreach.14 red; removing both takes (the one
in `unsuppressed`, which every barrier calls, and the one in
`lower_inlined_body`) turns all six red plus supval.21, noreach.7, .8, .10,
.11, .13 and .16. supval.7, .8, .18-.20 and noreach.5, .9, .12 are the
`$(- X …)` shapes, which the flag design and the deleted scope cover. They
fail on the pre-unit binary, which is their negative control.

Decide-and-report (DG may veto): the rule's reach as listed, including a
stage's operand capture aborting only its stage child. Not changed here,
and recorded as questions in `notes/tasks/q-suppression-reach-residuals.md`:
an assignment's own captures (`- x=$(cmd)`, `- x=$(strlen $(cmd))`,
`- a[$(cmd)]=v`) are still discarded; the store of `- (( n = … ))` is
lowered as a suppressed assignment, so a capture in its value is discarded
too; `$(- value_intrinsic …)` has no status of its own to discard; `-` on
`x=$(void_f)` escapes SB4076; and a `-` on a compound still covers the
compound's own direct commands (`retire-suppression-on-compounds`).

Specs: errors.sh noreach.1-noreach.16. Rewritten to the rule: errors.sh
supval.7, supval.8 and supval.18-supval.21. Journal 548.

**Addendum 2026-10-30 (F6 unit C, batch-531 B84).** A suppressed `||` list
consumes its status the way a condition does. Under `-` the right operand
runs unchecked, so the top-level reset of `$?` to 0 after `||` no longer
applies, and the list ends with its right side's status. Before,
`- /bin/false || /bin/false` exited 0; now it exits 1, as a lone
`- /bin/false` does. This is a bug fix, not a new rule. Two questions stay
open in `notes/tasks/q-suppression-reach-residuals.md`: R6, whether a
statement `-` should bind only the first operand (today it covers the whole
list, while a pipeline-head `-` binds stage 1 only), and R7, SB4076 not
looking through a list. Specs: errors.sh 55.145-55.147. Journal 549.
