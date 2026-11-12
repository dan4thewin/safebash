# 628 - Let named predicates supply stdout and status together

DG questioned DD88's capture/branch asymmetries after the decision-principles
pass. The original external/pipeline-only scope was deliberately narrow;
DD101 later admitted `getenv` to distinguish absent from present-but-empty
input. Neither history nor the existing result-kind model supplied a strong
reason to exclude a named predicate with both stdout and an explicit status.

DG then asked:

> I want to allow a predicate function to work like external/getenv - issues?

The discussion identified no fundamental semantic obstacle, provided the
existing result, body, taint and process boundaries remain. DG authorized
recording, rule updates and an implementation backlog item. This entry
records that accepted policy, not compiler delivery.

## The rule

A call legal as `if f`, with an otherwise-legal stdout capture, can be a
capture-condition: bind stdout and branch on f's explicit returned status.
Use the existing untyped consumers uniformly, including loops, negation,
logical lists and otherwise-legal redirects.

Invoke f once. On false, bind this attempt's output, including empty or
partial text, not the target's previous value. Preserve full status and
ordinary handling-position/negation behavior. The result is neither the last
internal command's status nor a test of stdout emptiness.

An outer condition handles the returned result, not failures inside the
body or operands. A bare unchecked command still aborts; `return command`
explicitly forwards its result. Fatal faults and `exit`/`fail` remain
terminal. Execution stays in-process, so global/collection effects persist
on false as on true. Success does not attest or erase taint.

Integer and void functions remain different result kinds. Anonymous bodies
do not gain explicit status returns or a general multi-statement condition.
Typed declarations and arbitrary expressions remain outside DD88's scope.
Outside a handling context, a predicate stdout capture still requires
explicit status discard. `getenv`'s uncaught-absence exception is not copied.

## Reconcile the rules, retain the boundaries

The AI updated DD88 in place, including its opening list, producer scope and
redirect account. Superseded blanket function exclusions are struck.
DD55/DD74 name the new consumer; DD86 distinguishes its named return from
an anonymous body's result. DD127's formerly struck conditional-adjacency
example now applies to eligible named predicates, without relaxing ownership
or failed-path resolution. Its stdout-iteration example remains struck.

DD101's old claim that nothing requires status consumption is struck and
qualified by the current non-void rule. This is the relevant distinction
between the two producers, not a change to environment lookup.

The new P3 work item
[`predicate-capture-conditions`](../tasks/predicate-capture-conditions.md)
owns implementation and native/C proof. The anonymous-return, discarded-status,
file-failure and diagnostic tasks now preserve the same eligibility boundary.
The resolved ceremony question remains closed. No unrelated task is completed
or made a prerequisite merely because it shares a capture path.

## Evidence and delivery boundary

The bounded source read at `228921e2` found separate direct/body-form
function exclusions in `try_capture_condition` and
`capture_condition_head_is_user_fn`, plus the existing condition/suppression
reset in `lower_inlined_body`. These explain the implementation obligations;
they are not an execution result or a claim that removing one guard suffices.

The backlog requires actual kind classification, single invocation,
full-status preservation, this-attempt publication, same-process effects,
body isolation and unchanged taint/handler behavior. Existing function
refusals must be inventoried by kind before changing specs: the new permission
does not retire void/integer refusals.

Only decision/task/journal/status Markdown changes. No compiler, runtime,
spec, build rule or shipped architecture is changed; no new behavior probe,
spec sweep or coverage measurement is claimed. The last recorded gate and
its generated status block remain the previous actual snapshot.
