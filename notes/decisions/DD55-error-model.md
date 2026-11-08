title: DD#55 — Implicit Exit Checks and Error Handling Model
status: implemented

No `set -e` at runtime. The compiler inserts implicit exit checks for
recoverable operational failures: nonzero command status or a runtime error
such as `regcomp` on a dynamic pattern. DD130 distinguishes these from
fatal faults, which abort rather than produce a resumable result.
`- cmd` discards that operation's result, including a recoverable failure
of its attached redirect; it does not suppress every failure in its
operands or body.

**DD130's 2026-11-06 classification extension is accepted, with
implementation pending.** It replaces the former setup-position split:
fork failure is fatal, while path-specific file errors are recoverable.
The existing runtime/handler implementation is not yet a claim that every
form enforces this contract.

## Terminal handlers (DD#128)

An uncaught main-process failure enters **ERR**, not EXIT. Normal
fall-through, every explicit `exit [N]` and a configured EXIT signal enter
**EXIT**. Explicit `fail [N]` enters ERR. Defaults are 0 for `exit` and 1
for `fail`; `fail 0` is legal. Handler selection is independent of the full
i64 status, which is preserved until process exit truncation (DD#89).
Children run neither handler.

Both builtins are terminal operations, not returned statuses that `if`,
`||` or `-` can consume to resume execution. An `exit` inside either handler
terminates directly; a `fail` inside EXIT enters ERR, while a `fail` inside
ERR terminates without re-entry. This 2026-11-06 revision was accepted in
journal 575 and implemented in journal 576.

EXIT is ordinary checked code: its uncaught failure enters ERR. ERR must
catch or suppress every recoverable failure where it actually occurs;
otherwise SB5101 rejects the operation or call. This is not a blanket
unchecked region. The no-reach rule below applies unchanged to helpers,
and operand captures; attached redirects follow DD130's classification.
If no existing form catches a recoverable failure, that operation is
unavailable in ERR until its handling support is implemented.

Arithmetic faults, heap exhaustion and fork failure are fatal faults.
A first fault in main or EXIT attempts ERR once without allocating for dispatch;
a fault in ERR terminates. Neither handler resumes interrupted code or
re-enters itself, and ERR cannot transfer back to EXIT. Falling off a handler
preserves its entry status; explicit `exit N` or `fail N` replaces it as
described above. `$?` is available as the first evaluated expression on entry
under the usual handling rule.

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

The operation's recoverable attached-redirect failure is discarded too
(DD130). A fatal fault is never discarded or inverted into a normal
result. Function result-kind restrictions and the no-reach rule remain.

### Command adjacency (DG, 2026-11-08)

DG: **"reject the - not adjacent to the command"**. A source `-` must
directly prefix the command or call whose result it discards. It cannot
prefix an assignment and reach through that binding to a command in its
value or index:

```bash
- x=$(/bin/false)    # rejected, not an alias
x=$(- /bin/false)    # discard /bin/false's status, capture its stdout
```

The rejection also covers append and cell assignments (`- x+=...`,
`- a[i]=...`, `- a[i]+=...`), whether or not their value contains a
capture. Declarations retain their existing refusal. DD71's value
bindings do not acquire a discardable result from an operand.

A prefix belongs to one command, not a whole `&&`/`||` list. In
`- a || b` or `- a && b` it belongs only to `a`; it cannot discard `b`'s
result. To discard `b`, put a separate `-` directly before `b`. A
pipeline-head `-` likewise belongs only to its first stage. Binding and
consumption are distinct checks: the subsequent ruling below rejects
`- a || b` and `- a && b` because their left status is consumed.

Adjacency is necessary, not sufficient: existing result-kind and
context restrictions still apply. `[[ ... ]]` and `(( ... ))` are single
commands, not compound bodies; this ruling does not decide the remaining
inert-prefix questions. Their operand captures remain checked, including
inside an arithmetic write. A `-` on that arithmetic command cannot
suppress a command nested in its expression.

The existing ban on compound prefixes remains
`retire-suppression-on-compounds`. DD88/DD101 capture conditions remain
legal handling forms; they do not license an outer assignment `-`.
No body, operand, taint or fatal-fault boundary changes.

**Enforcement pending:** `reject-nonadjacent-suppression` owns assignment
refusals, command-local list binding/classification and the remaining
arithmetic-operand reach repair. Journal 588 records this policy-only
change.

### A discarded status cannot be consumed (DG, 2026-11-08)

DG chose **reject** for testing a result after `-` discards it. A source
`-` removes that command's result from the available status producers;
it does not supply an always-successful replacement or leave the original
status available to a consumer.

Reject a directly suppressed command in `if`, `elif`, `while` or `until`,
and wherever its status decides an `&&`/`||` continuation. This includes
middle clauses and the final clause when the whole list is a condition.
For example, both `- a || b` and `if a || - b { ... }` reject. A final
`a || - b` at statement level remains legal: nothing consumes `b`'s
result. Existing result-kind restrictions still apply.

Capture conditions obey the same rule: `if x=$(- cmd) { ... }` rejects.
Remove `-` when the captured command's status is needed. A pipeline used
as a condition must have at least one stage whose status was not
explicitly discarded. Thus an all-suppressed condition pipeline rejects,
including inside a capture condition. A partially suppressed pipeline
remains legal: the condition uses the aggregate over its unsuppressed
stages, not the excluded statuses.

DD74's `return <producer>` explicitly consumes its producer's status too.
It cannot forward a discarded status, including a suppressed final list
clause or an all-suppressed pipeline. A body may still discard a command
and then return a fresh authored status, such as `- cmd; return 0`.
This is not a ban on constant results or on discard inside a predicate
function's body.

This is not a blanket ban on `-` anywhere inside a condition. A legal
`$(- cmd)` used as a text operand discards status but supplies stdout;
testing that text consumes the enclosing test's boolean, not the
discarded status. Normal taint, type and body-boundary rules remain.
Ordinary discard, stdout captures without a status consumer and
all-suppressed pipelines at statement level remain legal.

Handling and discard are not interchangeable compiler flags: an ordinary
condition or return producer disables an implicit abort because it
**consumes** status, not because the author wrote `-`. Preserve those
forms, existing `!`/`-` refusals and `$?` availability rules. This does
not reset process exit bookkeeping or child outcomes. In particular,
`- wait` discards its pid, not DD126's separate `waitstatus` output.

**Enforcement pending:** `reject-discarded-status-consumers` owns the
source-level refusals and preservation controls. Q3 in
`q-suppression-and-stage-residuals` is settled as a policy question;
journal 589 records the decision. No new diagnostic code is selected here.

### No inert prefix on a value-intrinsic capture (DG, 2026-11-08)

DG rejected `-` on a captured value intrinsic that has no separate
status to discard:

```bash
str x=$(- toupper abc)    # rejected
str x=$(toupper abc)      # consume the string value
```

The capture consumes the intrinsic's value. If that is its only result,
`-` has nothing left to discard. This applies to string and integer value
intrinsics, including an otherwise valid use inside an argument or
interpolation. Neither wrapping the capture nor attaching redirects
creates another result channel; existing placement and redirect-syntax
restrictions remain.

This is a result-channel rule, not a test of whether particular arguments
make failure likely. DD101's `getenv` has a real lookup status in addition
to its value, so it is not in this class. Its legal status discard and
ordinary capture-condition handling remain, with the consumer rule above.
An external command's status is likewise distinct from captured stdout.

Nor does this ban ordinary explicit value discard: `- int_fn` discards
that call's integer, and `- wait` discards its returned pid (DD126).
Neither form simultaneously captures the value it discards.
The child outcome saved in `waitstatus` is not a status returned by `wait`.
No new statement-position meaning is granted to value-only intrinsics.

Operand and body failures retain the no-reach rule. Heap exhaustion and
other fatal faults remain fatal under DD130; they do not supply a status
for this prefix to discard. Recoverable attached-redirect handling still
requires an otherwise legal handling form and does not change result kind.

**Enforcement pending:** `reject-statusless-intrinsic-suppression` owns
the refusal and preservation controls. R3 closes the last policy question
in `q-suppression-reach-residuals`, not its queued implementation work.
Journal 590 records the ruling. Control-word/no-op prefixes and discarded
tests remain separate questions in `q-suppression-and-stage-residuals`.

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

### Evaluation order (DG, 2026-11-06)

DD86 now requires left-to-right evaluation of words and their component
expressions. A later capture cannot run ahead of an earlier `$?` read:
`cmd || echo "[$?]" "$(f)"` reads the status that entered the branch before
calling `f`. Reversing the arguments makes `$?` no longer the first
expression, so it is SB5012, not permission to read the capture's status.

This ruling is accepted but its enforcement remains pending in
`q-capture-hoisting-order` (journal 580). Capture entry state is separately
settled below: evaluation order does not grant a body its caller's status.

### Capture bodies do not inherit status (DG, 2026-11-07)

DG chose **no inheritance**. An anonymous capture body starts with status
unavailable, like a named function body. It cannot read the caller's `$?`,
even when the capture is the first expression of an outer handling branch.
Only the body's own flow can make a status read legal under this DD.

```sash
/bin/sh -c "exit 3" || echo "$(echo "$?")"  # required: SB5012
```

The boundary applies at every nested executable capture body, whether it
appears in an assignment, argument or interpolation. Redirects and outer
conditions or suppression do not grant entry permission. A handling branch
established inside the body still receives its own status normally.

When the caller's status is needed, read it at a legal outer handling
position into an ordinary binding, then use that binding inside the
capture. DD86's lexical access and global effects are unchanged. Fresh
availability is not a fabricated zero result or subshell-style state
isolation.

The direct value intrinsic `$(status)` is a read in its surrounding
context, not an anonymous executable body: `/bin/false || echo "$(status)"`
still reads that failure's status. `$?` and the arithmetic spelling of
`status` obey the same availability rule. Putting any spelling inside an
executable body cannot bypass its fresh entry state.

DD126's fresh-function-context rule for `$!`/`$(getcpid)` and
`$(waitstatus)` applies to anonymous bodies too. The body's own spawn/reap
operations can establish fresh latches; a caller's latch cannot be borrowed.
Direct intrinsic value reads retain their surrounding context.

**Accepted, implementation pending** in
`bug-sb5012-sb5094-nested-body-flow` (journal 586). That task also owns the
existing errors caused by checking a body's internal reads against its
enclosing statement. This ruling does not change function result kinds,
capture-condition legality or the rejection of anonymous status returns.

### Attached redirects follow arguments (DG, 2026-11-07)

DD86 requires arguments to be evaluated before the command's attached
redirects, in standalone commands and pipeline stages alike. Targets are
then evaluated and redirects applied in redirect order, before the command
body runs. Stage arguments remain in the child (DD122).

An outer condition or `-` still does not handle failures inside argument
captures or value calls. A recoverable redirect failure skips the command,
not the argument effects that already happened; DD130's failure classes
and restoration requirements remain. Enforcement of the timing rule is
pending in `q-stage-capture-redirect-order` (journal 585).

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

An int-valued function is not a status producer and cannot be called
directly as a stage, including with `-` (SB4030, DD#58). Suppression does
not make an ill-typed stage legal. This type boundary was settled by DG
on 2026-11-05 and implemented in journal 577
(`q-int-function-pipeline-status`). The rules below apply to legal stages.

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

The 2026-11-08 consumer rule requires a nonempty set of unsuppressed
stages when that aggregate is consumed. An all-suppressed pipeline is
still a legal statement, but not a condition or status return producer.
This changes which forms are admitted, not the aggregation of legal
partially suppressed pipelines.

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
DD130 determines the failure's class before any catching or suppression
decision. Fork failure is fatal; other construction failures must be
classified by their actual cause. A fatal construction failure cannot
become a false condition or a suppressed success after cleanup.

Both backends check allocation/fork results before consuming pipe outputs or
recording a pid. A failed capturing pipeline skips its reader/loop entirely;
construction failure is not EOF and must not clear or overwrite the consumer's
target. Child fd wiring or redirect failure prevents that stage's body from
running.

A single external `$(command)` uses the same checked pipeline construction.
Uncaught recoverable failure enters ERR; a caught recoverable failure keeps
the capture's actual output/status contract, including empty output when
the command never ran. `$(- command)` may discard that recoverable result,
not a fatal construction fault. Failed setup never executes the command.
The capture drains its private pipe before waiting and does not replace an
enclosing lastpipe reader. The lifecycle machinery is implemented; the
DD130 failure-class routing remains pending.

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

A failed fork is fatal in the parent (DD130), not a child's returned status.
An exec failure occurs in the child; the parent observes the child's
completion under the existing process-status rules (details in the
2026-10-26 addendum).
A successfully forked background process runs independently and
can outlive the sash program. `wait` is optional.

**Amended by DD#126 (2026-10-09).** `wait` no longer returns a status and is
no longer subject to the implicit check: it is an int intrinsic yielding the
reaped **pid**, or `-1` on `ECHILD`, and it never aborts. The reaped child's
exit is read through `$(waitstatus)`. A bare `wait` statement is SB4016 — the
unconsumed-value rule, not the implicit-check rule.

## User-defined functions

Each function name has one source definition (DD#86; DG, 2026-11-06).
A repeated definition is an error regardless of body, signature, result
kind or use. Duplicate rejection is accepted but not yet implemented;
`reject-duplicate-function-definitions` owns enforcement.

### Return kinds (DG, 2026-11-06)

Return kind is independent of the numeric value returned:

| Kind | Normal returns |
|---|---|
| Void | Bare `return`, fall-through, or forwarding a void callee |
| Status | An explicit status on every returning path |
| Integer (`-> int`) | An explicit integer on every returning path |

Bare `return` supplies no result; it is not shorthand for `return 0`.
Status and integer functions may neither use a bare return nor fall off
the end on a returning path. Mixed result kinds are errors, regardless of
whether a caller uses the function as a predicate or discards its answer.
Paths that terminate the process are not returning paths.

Compatible `return g` forwarding remains legal and preserves g's result
kind, including through a redirect. A self-tail call is a loop edge, not
an exit that supplies zero. No implicit void/status/integer conversion is
introduced by forwarding.

**Constant results are legal.** There is no same-result check, and the
int-only "returns 0" ceremony rule (SB4017) is retired. In an unannotated
function, `return 0` produces a status; in an `-> int` function it produces
an integer. Neither means void, even when it is the only result.

Implemented together in journal 577 (`enforce-function-return-kinds` and
`retire-return-ceremony-check`). Journal 574 records DG's ruling.

### Void functions

Bare returns, fall-through and compatible void forwarding supply no result.
Bare call at the call site is fine; an in-process `-` has nothing to discard.

### Non-void functions

A status or integer result makes a function non-void. Call sites must
consume or explicitly discard that result:

- `if myfunc` / `myfunc || handle` — handling a status, not an integer
- value capture or arithmetic composition — consuming an integer
- `- myfunc` — discarding the function's own answer
- bare `myfunc` — **compile error** (unhandled non-void return)

These call-site obligations do not disappear for a constant result.
`-` still does not reach into the body or repair a type mismatch.

### Anonymous capture returns (DG, 2026-11-06)

An anonymous `$( ... )` body may not explicitly return a status. Reject
`return 3`, `return 0`, `return $?` and status-producing forwarding such as
`return status_f`, a command, test, pipeline or list producer. Redirects
and nesting inside the body's control flow do not change that result kind.
The status must not be silently dropped, converted into the captured value,
or given an implicit failure check in place of the required rejection.

Bare `return`, fall-through and compatible void forwarding remain legal:
they supply no result. This is a result-kind restriction, not a ban on the
`return` keyword or on constant results. Process termination is not a return.

If a body needs to return a status, give it a name. Its ordinary call sites
must handle or discard that status; `$(- status_f)` captures its stdout and
explicitly discards its returned status. A named function called from an
anonymous body keeps its own return contract. A `-` on the first statement
inside a capture discards only that statement's result, not the body's return.

No anonymous-result handling syntax is added. Direct integer captures
(DD#58), ordinary command/pipeline captures and DD#88/DD#101's existing
capture-conditions are unchanged; user-function and multi-statement
capture-conditions remain excluded.

**Accepted, implementation pending** in
`reject-anonymous-capture-status-returns` (journal 584).
`q-status-function-ceremony` is closed as a policy question, not as a fix.

### Process replacement inside captures (DG, 2026-11-08)

DD86 rejects source-level `exec PROGRAM ...` inside a stdout-capture body,
directly or through called functions. Suppression or a handling context
cannot make that operation legal. Ordinary external-command captures and
fd-only `exec >file` remain legal under their existing rules.

This is a source restriction, not a new failure class or a change to
DD128's `exit`/`fail` routing. Enforcement remains pending in
`q-exec-in-capture` (journal 587).

### Bare call of int-typed function

Compile error. Capture the integer, use it in arithmetic, or explicitly
discard it with `-`. A bare call silently discards the value; using the
function directly as a predicate or pipeline stage is a type error.

An integer-function capture consumes only the returned integer, including
through redirects (DD#58; DG, 2026-11-06). `$(- f)` and `$(- f 2>&1)` are
both SB4076: redirecting I/O does not make the already-consumed integer
available to discard. Stdout remains a legal side effect. A non-integer
wrapper can discard the integer with a statement `- f` and expose stdout
through its own capture. Redirect-preserving enforcement is pending in
`bug-int-capture-redirect-switches-kind`.

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

**Background failure ownership.** The exec fails in the child, which prints
the errno and "command failed (exit 127)" and exits 127. The parent sees the status only
by reaping it through DD#126's latch (55.16). Under `- cmd &` and `exec cmd &`
the child prints the errno but not the "command failed" line. F2
(2026-10-28, batch-531 B20, journal 539) implemented background fork failure
as `fork: error N`, status 1, abort unless checked or suppressed, with `$!`
set to -1. DD130 now requires a fatal path instead, so no handling branch
continues after the failed fork. That change remains in
`bug-fork-failure-reporting-by-form`. DD128's main-process-only handlers
and once-only fatal dispatch remain; END's old silent exception is gone
(historical row e1).

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

This records the F6 implementation of the 2026-10-29 and 2026-10-30 rulings
in `q-suppression-on-user-functions` (Q3, and Q2 for in-process calls), and
batch-531 B15. The contract is kept current below; the 2026-11-06 return-kind
and ceremony changes are implemented in journal 577, as stated under
User-defined functions.

**Three classes.** A function returns an int (`-> int`), a status, or
nothing (void), under the kind-based contract above. Journal 577 replaces
F6's numeric/bare-zero classification with that contract. A forwarded result
keeps its kind; `return g >file` does not turn a void g into a status
producer. Incompatible forwarding, including int-to-status, is a type
error, enforced by `enforce-function-return-kinds`.

**One definition per name.** DG's 2026-11-06 ruling replaces F6's
definition-in-force behavior: duplicate source definitions are rejected,
including same-kind and identical definitions (DD#86). Every call and
template must describe the same single body and its result kind.

Enforcement remains pending in `reject-duplicate-function-definitions`.
F6 currently recomputes the classes as definitions are lowered, and its
template table excludes redefined names and their dependents.
`nonvoid.25`-`nonvoid.29` record that old behavior, not an exception to the
new rule; migrate the affected cases when enforcement lands.
Preserve classification of forward references to unique definitions
(`nonvoid.9`). The name-based integer-call miscompile is tracked by
`bug-int-call-by-name-redefinition` until its duplicate-definition
reproducers are rejected.

**A status function's status is handled or discarded (B15).** A call is
handled as a condition (`if`, `while`, the left side of `&&` or `||`), or
its status is discarded with `-`. Anywhere else it is SB4016, "function 'f'
returns a status — handle it with `if` or `||`, or discard it with `-`": a
bare statement, the tail of a list (`/bin/true && f`), a background job
(`f &`), in a function body or in EXIT/ERR. Reading `$?` afterwards does not
handle it. A capture `$(f)` binds the output and leaves the status nowhere
to go, so it is SB4016 as well, with the advice `$(- f)` (`$(- f …)` when it
has arguments). A capture as a condition, `if x=$(f)`, stays SB4025 (DD#88),
and the SB4016 at the same position is dropped. A redirected capture of a
status function, `$(s 2>&1)`, still needs explicit status handling/discard;
the current statement path gives it the bare-call text. This is not a rule
that a redirect converts an integer capture into a stdout capture (DD#58).

**`-` on a void function is SB4076** (new): "function 'f' returns nothing,
so `-` has nothing to discard — call it without `-`". A capture of an int
function binds the int, so `$(- f)` on one is SB4076 too: "`$(f)` binds the
int that function 'f' returns, so `-` has nothing to discard — capture it
without `-`". The same refusal is required with attached redirects, such
as `$(- f 2>&1)`; that extension is pending in
`bug-int-capture-redirect-switches-kind`. A stage or job has its own process
status, so `- v | /bin/cat` and `- v &` stay legal for a void v (nonvoid.24).

**Return completeness replaces ceremony.** SB4017 is retired by DG's
2026-11-06 ruling. An explicit integer zero is valid, including alongside
integer forwarding; forwarding a void result into an integer function is
still invalid, but because its kind is wrong. The new kind/completeness
guards must be in place before removing the old check, or in the same
change. Journal 547 records the original guard and its specs; the removal
task is `retire-return-ceremony-check`.

**Deleted.** The suppressed int self-tail specialization (journal 530)
lowered `return $(- f …)` inside an int f as a second, suppressed copy of
the loop. That spelling is now SB4076, so nothing reaches it. It is gone,
with `TailLoop.entry_readonly`.

The other F6 call-site choices remain: `f &` as SB4016, SB4025 superseding
SB4016, SB4076 for `$(- int_f)`, and their advice texts. Definition-in-force
replacement is superseded by DD#86's unique-definition rule. Redirects
preserve both a forwarded result's kind and an integer capture's result
channel; their implementation owners are identified above. The following
F6 unit settled whether `-` reaches into a body or an argument capture.
`notes/tasks/q-nonvoid-classifier-residuals.md` now records the resolved
questions and the still-open enforcement tasks.

Specs: errors.sh nonvoid.1-nonvoid.29. Rewritten to the rule: errors.sh
rdr.39, supval.1-supval.6, supval.9-supval.12, supval.14-supval.17,
supval.22 and supval.28; functions.sh 74.55-74.60, 74.64b, 74.67, tmpcap.19
and tmpcap.20; coverage.sh cov.330; pipe_functions.sh pf.71; taint.sh
div.2; types.sh constw.19. Journal 547.

## Addendum (2026-10-30, batch-531 port F6 unit B): a `-` does not reach

DG ruled on 2026-10-29 that a `-` on a call discards only the function's
returned value (`q-suppression-on-user-functions` Q1 and Q4). Taken with
DD#86 (a body is a function body, checked once), the rule for every `-` is:
**a `-` discards the returned answer of the one command or call it prefixes,
and nothing else.** Discarding an integer in a value context does not
convert it to a status or make a status-only context legal (DD#58).
These are checked as if the `-` were absent:

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
stage's operand capture aborting only its stage child. F6 left assignment
prefixes, arithmetic-write operands, status-less value intrinsics and
compound prefixes unchanged; the historical probes remain in
`notes/tasks/q-suppression-reach-residuals.md`.

DG's 2026-11-08 adjacency ruling above rejects the assignment prefixes,
including the void-function-capture route, and the no-reach rule requires
arithmetic-write operands to stay checked. Enforcement is queued in
`reject-nonadjacent-suppression`; compound refusal remains separately
queued in `retire-suppression-on-compounds`. DG also rejected R3's
status-less value-intrinsic captures; enforcement is queued in
`reject-statusless-intrinsic-suppression` (journal 590). The historical
noreach.12 spelling needs review when that rule lands: a refusal for an
inert prefix cannot stand in for proof of the operand barrier.

Specs: errors.sh noreach.1-noreach.16. Rewritten to the rule: errors.sh
supval.7, supval.8 and supval.18-supval.21. Journal 548.

**F6 unit C history (2026-10-30, batch-531 B84).** Journal 549 repaired
status retention on the then-accepted whole-list suppression route: its
unchecked right operand's status was no longer reset to zero.
`- /bin/false || /bin/false` changed from exit 0 to exit 1. Specs
errors.sh 55.145-55.147 record that repair, not an enduring permission to
suppress a whole list.

DG's 2026-11-08 rule closes R6's binding question: the prefix belongs to
the adjacent command only. Result-kind checks must inspect that command,
not an unprefixed later operand (R7). DG also rejected consuming the
discarded status (Q3, journal 589), so these leading-prefix logical lists
must reject. The adjacency and consumer enforcement tasks must coordinate
review of 55.145-55.147, use fresh ids for changed contracts and preserve
genuine status-retention coverage on legal routes.
