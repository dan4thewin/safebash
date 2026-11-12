title: DD#88 - Capture-conditions bind values and consume producer results
status: accepted

1. An untyped single-producer binding may consume the actual status of a single
   external command, named predicate, otherwise-legal pipeline, `getenv` lookup
   or direct file slurp. Capture preserves pipeline-stage eligibility.
2. Invoke a named predicate once: bind its stdout and consume its explicit
   returned status. Same-process effects persist on either result.
3. Bind this attempt's bytes on false too, including partial/empty text;
   a completed empty read succeeds. Handling is not attest.
4. Assignment itself has no result. Declarations, void/integer function calls,
   arbitrary expressions and multi-statement bodies gain no condition result.
5. Ordinary handling-position, negation, taint and no-reach rules apply;
   discarded statuses cannot also be consumed.
6. Stdout captures must retain a possible connected writer after redirects;
   direct integer captures consume an integer and are outside that writer test.

---

The external-command form was settled with DG on 2026-09-02 and implemented
the same session. DD101 later added the implemented `getenv` lookup form.
DG accepted direct file-read capture-conditions on 2026-11-10:
**"yes to capture-condition"** (journal 610). That extension is not yet
implemented; `recoverable-file-failures` owns it.
DG accepted named predicate-function capture-conditions on 2026-11-12
(journal 628). That extension is also **unimplemented**;
[`predicate-capture-conditions`](../tasks/predicate-capture-conditions.md)
owns it.

These are scoped forms, not general multi-value binding. The general
multi-value `return` / `(stdout, status, stderr)` trio remains deferred to
`multi-assignment-and-capture-trio`. The regex extension below is provisional.

## The gap

Originally, capturing a command's stdout **and** branching on its exit status
had no working form (see the `multi-assignment-and-capture-trio` motivation table):
`if ! out=$(cmd)` was SB4025; the "sequence it" workaround aborts on failure;
DD#55's `out=$(cmd) || …` idiom was itself SB4025. Root cause: DD#71 treats an
assignment as a value binding with "no meaningful exit status" — true for
`x=foo`, but `x=$(cmd)` carries the command's status, which the idiom needs.

## The decision

In an existing **boolean/status-consuming context**, an untyped assignment
whose RHS is one eligible producer is a **capture-condition**. The accepted
producers are a single external command, an ~~external~~ otherwise-legal
pipeline, a named predicate function, DD101's `getenv` lookup, and a direct
`$(<file)` read:

- the **condition consumes the producer's actual status**
  (`test = status == 0`); a recoverable nonzero result does **not** abort
  (the status is consumed — DD#55's "binding the status" rule);
- the producer's **text is bound to the target**. External output, environment
  data and file input retain their taint; handling acquisition failure is
  not validation of the acquired data;
- `$?` is available as the **first expression of the handling branch** (the
  existing DD#55 handling-position rule), so `if x=$(cmd) { … } else { echo $? }`
  reads the real failing code in the else branch;
- `!`, `else`, and `$?`-first-in-branch behave **identically to a plain command
  condition** — in particular the negated spelling `if ! x=$(cmd) { … }`
  replaces `$?` with the negated boolean (0/1), losing the magnitude, exactly
  like `if ! cmd`. To read the numeric code in-branch, use the non-negated
  `if x=$(cmd) { use $x } else { echo "failed: $?" }` form.

### Scope

- **RHS shape:** a single external `$(cmd)` OR a pipeline `$(a | b)` ~~of external
  stages~~, a named predicate `$(f args...)`, `$(getenv NAME)`, or a direct
  `$(<file)`. ~~Excluded (stay SB4025): a user-function capture `$(f)`
  (functions convey status via `return` — DD#74 predicates).~~
  Excluded: void/integer function calls, other value-only intrinsics,
  a concatenation such as
  `foo$(cmd)`, a multi-statement `$(a; b)`, and any literal RHS.
- **Untyped only:** `if int n=$(cmd)` (typed declaration) is NOT a
  capture-condition — declarations stay SB4025.
- **Existing consumers:** `if`/`elif`, `while`/`until`, negation and logical-list
  conditions use the same producer-result rule.
- **Stdout on a returned failure:** this attempt's partial/empty bytes remain,
  including for a named predicate.
  File reads follow the corresponding rule below; `getenv` retains DD101's
  lookup-value contract. A truly-unused captured target is a dead variable
  (SB5050). For an external command whose stdout is not needed, use the
  non-capturing `if ! cmd { … }` form.
- **`$?` after:** no new rule — the existing DD#55 handling-position machinery
  applies (a capture-condition assignment provides `$?` like a command).

Capture does not narrow an otherwise-legal pipeline's stage eligibility
(DG, 2026-11-12). User functions behaving as filters remain legal stages;
the condition consumes the pipeline's aggregate. Existing stage/result-kind,
connected-writer, taint, no-reach and fatal-failure rules still apply.
~~This does not admit a direct `if text=$(f)` capture condition.~~
Direct named predicates are separately admitted below; no anonymous status
returns or purity/filter annotation are added.

DD#55's anonymous-status-return rejection (DG, 2026-11-06) adds no new
anonymous capture-condition form. ~~A function or multi-statement capture
does not become a condition by containing an explicit status return.~~
A named status-returning function qualifies as specified below; an anonymous
body does not gain a result by containing a `return`. The eligible producer
forms above do not admit arbitrary executable capture bodies.

### Named predicate capture-conditions (DG, 2026-11-12)

A call legal as `if f`, whose stdout is otherwise legally capturable, may
supply both results: `if text=$(f args...)` binds its output and branches
on its returned status. This applies to every existing capture-condition
consumer, including loops, negation and logical-list decisions, and to
otherwise-legal redirected calls. No new function annotation is needed.

Invoke the function **once**. On return, use its explicit DD74 status:
zero is true, nonzero is false. Do not test the output's emptiness or the
last internal command's status. Preserve the full returned status for the
existing handling-position `$?`; negation retains its existing 0/1 rule.
A false return still binds this attempt's stdout, possibly empty or partial,
not the target's previous value. Existing capture byte/newline handling stays.

The condition handles the function's returned result, **not failures inside
its body or operands**. An unchecked command inside `f` still aborts;
`return command` explicitly forwards that command's result. Attached
redirects retain DD130's failure rules. Fatal faults and `exit`/`fail`
remain terminal, not false returns.

Execution stays in the existing same-process model (DD86). Global and
collection effects persist on either returned result; capture introduces
neither a subshell nor rollback. Success is not attest: DD127/DD128 still
govern buffer/kernel-read taint, ownership, adjacency and legal resolution.
Writer and process-replacing-`exec` restrictions remain.

This is a new consumer for a named predicate's existing status, not a result
on assignment or an anonymous body's return. Integer/void calls, typed
declarations, concatenations and multi-statement bodies remain ineligible.
Outside a handling context, a status-function stdout capture still requires
explicit discard (`text=$(- f)`); DD101's permission to ignore lookup absence
belongs to `getenv`, not to functions. Conversely, `if text=$(- f)` cannot
consume the status that source `-` discards.

**Implementation pending:** `predicate-capture-conditions` owns eligibility,
result plumbing and native/C behavior. Preserve the existing shared
result-kind and body-boundary rules rather than merely deleting the
function-name rejection.

### Direct file-read capture-conditions (DG, 2026-11-10)

Use the existing capture-condition surface for the existing file-read
operation, without a new reader command or a fork through `cat`:

```sash
if text=$(<config) {
    attest text -n
    echo "$text"
} else {
    echo "read failed: $?" >&2
}
```

The read succeeds when it completes, **including an empty file**. Its result
does not test the length or content of `text`; the example's `attest` is a
separate content check. A recoverable open/read error supplies failure to
the condition. DD130's fatal faults remain fatal and do not enter a recovery
branch. Existing handling-position and negation rules apply to the real
operation status.

On a **handled failure**, bind the bytes read in **this attempt**, possibly
none. An open failure therefore binds empty text and reports failure, not
successful emptiness. A later read failure can leave partial text. Do not
retain the target's previous value or present partial input as a completed
read. Existing slurp byte/newline handling stays unchanged.

This is the scalar capture rule, not DD131's atomic array-fill rule. If the
target is an immutable scratch introduction in that separate trial, the
same-site execution creates its new instance; it cannot recover an older
instance's value or proof. No new scratch spelling is selected here.

An ordinary `text=$(<file)` remains implicitly checked **before assignment**.
It still aborts on an unhandled failure. Filename evaluation, helper bodies
and nested operand captures keep their own checks: handling the read does
not catch those failures. Taint, attest adjacency/ownership, scope and
result-kind rules are unchanged.

Only the direct eligible RHS gains this handling. No arbitrary string
expression, declaration or multi-statement body gains a
condition result. Assignment itself still has no result. This decision adds
no new suppression spelling such as `$(- <file)` and does not permit
`- text=$(<file)`. The external `cat` capture remains available.
Named predicates have the separate accepted extension above.

**Implementation pending:** `recoverable-file-failures`, with
`runtime-failure-classification`, owns the runtime result, native/C lowering,
source and ERR analysis, and independent failure/publication proofs.

### Provisional regex extension (DG, 2026-11-09; DD132)

The integrated DD131/DD132 trial adds capture-conditions for regex
operations that really supply **text plus a boolean**: scalar `match`
and the proposed `sub`/`gsub` result contract. For example,
`if _out=$(sub "$text" "$_re" replacement) { ... }` binds the text and
branches on whether a match occurred. For `sub`/`gsub`, no match supplies
valid original-subject text and false, not a failed/empty substitution fallback.
Runtime patterns require successful proof for their immutable values.

This is not a status on assignment itself, a scalar `matchall` form,
or permission for void/integer function, multi-statement, concatenated
or status-less intrinsic conditions. Existing declaration-position
restrictions remain; final scratch declaration spelling is an explicit
trial design item. To consume only text, discard the real boolean
inside the capture (`_out=$(- sub ...)`), not outside the binding.

DD131's fresh scratch lifetime also applies to an external capture
target, but does not overwrite this DD's partial-stdout rule: on failure,
this attempt's bytes remain, not the preceding instance's bytes.
Atomic array publication is not a new scalar-capture transaction.
Implementation and the DD revision inventory are in `q-glob-semantics`.

### Handling and discard (DD55)

DD55's command-adjacent `-` rule (DG, 2026-11-08) does not remove these
handling forms, including `if ! x=$(cmd)`. It rejects `- x=$(cmd)` as a
discard spelling; write `x=$(- cmd)` instead. A capture-condition is not
permission to move `-` outside the assignment.

DG also rejected consuming a discarded status (2026-11-08; DD55).
`if x=$(- cmd) { ... }` and an all-suppressed pipeline capture condition
must reject; the same applies in loop conditions and logical-list
consumers. Remove the source `-` when the command's status is needed.
A partially suppressed pipeline capture condition remains legal and
uses only its unsuppressed stages' aggregate.

Ordinary `if x=$(cmd)` and `if ! x=$(cmd)` remain handling forms. The
lowering's unchecked/suppressed flags do not make those source-level
discard: the condition consumes the status. Conversely, `x=$(- cmd)`
without a status consumer remains a legal stdout capture. Enforcement
is pending in `reject-discarded-status-consumers` (journal 589).

## Why the original shape

The implementation reuses two existing mechanisms:

1. Capture stdout and disable the implicit abort while retaining the
   command's status for the explicit handler.
2. `while read x` — an operation that binds a variable AND yields a condition
   truth value.

This is implementation sharing, not source equivalence to `x=$(- cmd)`:
an authored `-` discards precisely the status the condition needs.

## Original implementation

- **transform** (`transform/stmt_handlers.rs::try_capture_condition`): in
  `state.in_condition > 0`, a `CmdSubst` (single external) RHS lowers to a
  **suppressed** capture `Assign` + a `SetTestFromStatus` sigil; a
  `CmdSubstBody` that is a single `Pipeline` captures via `capture_body_into`
  with `state.suppressed` bumped (inner pipeline unchecked) + the sigil.
- **SSA**: new sigil `BuiltinCmd::SetTestFromStatus` (`__set_test_from_status`)
  — `test_result = ($? == 0)`, the sibling of `NegateTestResult`. A suppressed
  pipeline sets `test_result` itself; the sigil makes the single-command path
  uniform.
- **codegen**: native (`emit_set_test_from_status`) and c (`__test_result =
  (__exit_status == 0) ? 1 : 0`). Pipelines honor `state.suppressed` for their
  `checked` flag (`handle_regular_pipeline`).
- **validate** (`is_capture_condition_rhs`): the SB4025 carve-out. `validate`
  ~~runs BEFORE command resolution, so the command head is still bare — the
  external test is `resolve::is_external_looking` (excludes intrinsics +
  builtins), NOT a post-resolution `/` check. It cannot consult the function
  table, so a bare user-function name also passes here; the **transform**
  rejects the residual `$(user_fn)` capture-condition (SB4025, via
  `reject_nonexternal_capture_condition`) since it has the function table.~~ DD23
  requires resolution before semantic validation. The
  capture-condition also registers as a `$?`-provider in the SB5012 state
  machine. The transform path is guarded on a `CommandSubstitution` source expr
  so an arithmetic mutation `x=1` inside `(( … ))` (also lowered with
  `in_condition` set) is untouched.
- ~~**janet**: not implemented — janet emits from the IR (not the SSA sigils) and
  its `$(cmd)` capture is a stub. Positive-behavior specs are gated `native,c`;
  filed `janet-capture-condition`. The rejects (SB4025/SB5050/SB5020) are
  backend-independent and pass in all modes.~~ DD104 retires that backend.

## SB codes

No new codes. Reuses SB4025 (non-capture value binding in boolean context),
SB5050 (dead captured target), SB5020 (tainted target used without bless),
SB5012 (`$?` outside a handling position).

The original SB4025 message is context-aware: a plain value binding
(`if x=hi`) gets the "value binding — sequence it" advice; a `$(…)` capture that ISN'T a single
external command (a concatenation, or an intrinsic/builtin capture that can't
fail) gets a Group-B message that names the working form ("a capture-condition
branches on a single external command's exit status") rather than claiming the
feature is unsupported. ~~A `$(user_fn)` capture gets its own transform
message.~~ That blanket refusal remains in the compiler, but is superseded
for named predicates by the accepted extension above. Preserve meaningful
void/integer and invalid-shape diagnostics; no new code is selected here.

## Tests

`spec/capture_cond.sh` 88.1–88.18: success/failure branches, `$?`-in-else real
code, negated form, `$?`-first-in-then, pipeline + pipefail status, `while`
form, bare-external head (88.17), taint bless (SB5020), dead-var (SB5050), the
`!` vs `else` `$?`-magnitude contrast (88.15/16), and the exclusions with their
messages (~~user-fn /~~ concat / intrinsic / builtin / literal → SB4025).
Journal 236 records that original scope. Inventory function cases by their
actual result kind before changing specs: void/integer refusals remain,
while named-predicate refusals need new contracts at fresh ids.

## Deferred

The general feature — multiple-value `return`, multi-target binding, and the
external `(stdout, status, stderr)` trio — remains in
`multi-assignment-and-capture-trio`. The enumerated producer forms above
do not grant that general feature.

## Addendum 2026-09-12 — `getenv` joins the capture-condition

The original rule was restricted to **external** command substitutions,
on the grounds that a sash intrinsic "can't fail".
[DD#101](DD101-getenv-lookup-status.md) established the first intrinsic
exception: `$(getenv NAME)` interrogates external state and can report a
not-found **result**, so it composes with both `x=$(getenv N) || …`
and `if ! x=$(getenv N) { … }`. This lookup extension is implemented.
The accepted file-read extension and provisional regex extension are
specified above; value-only producers do not gain a separate result merely
by using `$()` notation.

## Addendum 2026-10-12 — redirects, and the capture that has no writer

DG, asked whether a redirect belongs in a capture-condition: *"so long as the
cmd's stdout remains connected to the caller (e.g., `cmd 2>&1 >f`) it should be
supported"* — and then, on seeing the rule written down: *"the idea that the
capture needs a writer even after redirects makes sense for all captures, not
just ones after `if`."*

So the rule is **not** limited to capture-conditions. It applies to every
**stdout-capturing** `$( … )`. DD#58's integer-value captures consume a
returned integer instead, including through redirects (DG, 2026-11-06).
They need no connected stdout writer; applying this check to them would
confuse two different result channels.

### The rule

Inside a stdout substitution the body starts with fd 1 attached to the
capture pipe.
A redirect list may move that pipe around. The capture is meaningful while
**some fd still holds it**, and nonsense otherwise — the value could only ever
be `""`. Simulated left to right, exactly as the kernel applies the list
(`ir::redirects_preserve_capture`):

| spelling | capture fed by | verdict |
|---|---|---|
| `$(cmd 2>/dev/null)` | stdout (fd 1 untouched) | legal |
| `$(cmd 2>&1 >f)` | **stderr** — fd 2 took the pipe before fd 1 left | legal |
| `$(cmd <in)` | stdout | legal |
| `$(cmd 3>&1)` | stdout, and fd 3 as well | legal |
| `$(cmd >f)` | nothing | **SB4075** |
| `$(cmd >f 2>&1)` | nothing — fd 2 dups the *file* | **SB4075** |
| `$(cmd &>f)` | nothing — the parser desugars this to `>f` | **SB4075** |
| `$(cmd 1>&2)` | nothing | **SB4075** |

A pipeline's capture is written by its **last stage**, so that is the stage
judged: `$(a >f | b)` is legal (`a` steals its own stdout from the next pipe,
not from us) and `$(a | b >f)` is not.

The check is permissive about what counts as a writer. **Any** top-level
statement of the body that could still write qualifies — `$(a >f; b)` is legal
because `b` feeds the capture — and a form whose writer is not plainly visible
(a compound, an assignment) counts as one. SB4075 only ever fires on a capture
that is unambiguously abandoned.

Fully decidable: `>&`/`<&` targets are literal fd numbers or the parser
already rejected them (SB4048), so there is no dynamic case.

The capture's result kind must be classified before applying this writer
test. For an integer function f, `$(f >file)` still consumes its integer
and routes its output to the file; it is not an abandoned stdout capture.
Enforcement of that distinction is pending in
`bug-int-capture-redirect-switches-kind` (journal 583). Real stdout
captures keep the SB4075 rule unchanged. This does not widen DD#88's
separate restrictions on which captures can be conditions.

### Why a new code rather than SB4025

SB4025 is "a value binding used where an exit status is consumed" — a
*boolean-context* fault. An abandoned capture is wrong in any position:
`a=$(cmd >f)` is the same mistake as `if a=$(cmd >f)`, and `echo $(cmd >f)` is
too. SB4075 is emitted from `validate_expr`'s `CommandSubstitution` arm, so it
fires wherever a substitution is written.

This is a **breaking change** against both bash and previous sash, which bound
`""` silently. It is the usual safebash trade: bash's answer is well-defined
and useless, and the program that wanted it is spelled `cmd >f`.

### What a redirect in a capture-condition now does

`try_capture_condition` previously matched two `SsaValue` shapes — a bare
`CmdSubst`, and a `CmdSubstBody` that `is_single_pipeline`. A redirected
command is neither (`single_command_subst` declines a redirected capture so the
redirect is lowered by the ordinary statement machinery), so it fell to
`_ => None`, whose only caller-side explanation is DD#88's user-function
rejection. **`if x=$(cmd 2>/dev/null)` was told that capturing a function's
output is not a condition, with no function anywhere in the program.**

In that implementation, a `CmdSubstBody` qualified when its single statement
peeled to a pipeline or ~~to a command that is not a user function — the
function-table check moves with the relaxation, so `$(f 2>&1)` stays on
DD#88's side of the line with its unredirected twin~~ a non-function command.
The November 12 extension requires direct and redirected named predicates
to qualify by their status result, not be excluded by name.
The body was lowered with `state.suppressed` bumped *and*
`uncheck_emitted_since` applied: only `handle_regular_pipeline` reads
`state.suppressed` at emission time, so a plain Command needs the post-pass to
be unchecked at all. Verified byte-identical to bash, including `$?` carrying
the real exit code into the `else` branch.

A second, smaller disagreement fixed alongside: `stmt_is_external_command`
peeled `-`/`!` but not `Redirected`, while the caller that peels a bare
command's redirect sat fifteen lines above it. `$(a | b 2>f)` drew a false
SB4025 on that account.

### Tests

`spec/capture_cond.sh` 88.19-88.28 (surviving redirects in `if`/`while`, the
`2>&1 >f` idiom with its file and its `$?`, fd 0, an unrelated fd, `-`
suppression, a pipeline stage, an earlier stage, and the original
user-function refusal) and cap4075.1-8 (the rejections, in condition / assignment
/ argument position, both orders of the idiom, `&>`, a dup, a pipeline tail,
and the multi-statement body that is still fed).

`spec/commands.sh` csr.1-csr.9 predate this and asserted the old
bind-empty-silently behaviour. The six write-to-file cases were respelled
`2>&1 >f`, which exercises the identical codegen — same literal / computed
target, same open, same dup2 — while keeping the capture fed, so the silent
redirect-drop bug they were written for stays pinned.
