title: Allow named predicate-function capture conditions
status: open
priority: P3
kind: work
depends: []
gates:
  - "an otherwise-legal stdout capture of a named status-returning function is accepted in every existing untyped capture-condition context, with or without legal redirects"
  - "invoke the function once, bind this attempt's stdout on either returned result, and branch on its explicit full status rather than output length or its last internal command"
  - "handling-position status reads and negation agree with plain predicate calls; capture completion and redirect restoration preserve the result"
  - "same-process global/collection effects persist, while operand/body checks, fatal faults and terminal transfers keep their existing boundaries"
  - "taint source classification, shared-output ownership, adjacency, failed-path resolution and handler analysis remain intact; success is not attest"
  - "void/integer calls, invalid RHS shapes, typed declarations and source-discarded status consumers remain refused without widening anonymous-return rules"
  - "ordinary status-function captures still require explicit discard outside a handling context; external, pipeline and getenv contracts remain unchanged"
  - "focused native/C specs have meaningful before/reversion controls; changed contracts use fresh global ids and the ordinary predicted gate passes"

## Accepted contract

DG requested a predicate function that works like external/getenv capture
conditions, then directed: **"record this, update rules accordingly, add
implementation to backlog"** (2026-11-12; journal 628).
[DD88](../decisions/DD88-capture-condition.md) is authoritative.

A call legal as `if f`, whose stdout is legally capturable, can supply both:
bind its output and consume its explicit returned status. Apply this to
`if`/`elif`, `while`/`until`, negation and logical-list consumers, not just
one branch spelling. Direct and otherwise-legal redirected calls agree.

Run the function once. Zero is true and nonzero false; preserve the full
status for existing handling-position reads. Bind this attempt's output on
false too, including empty or partial text, rather than retaining old target
data. Existing capture byte/newline handling stays.

No subshell, rollback, output validation or new function kind is introduced.
The condition does not handle failures inside operands or the function body.
`return command` explicitly forwards a status; a bare unchecked failing
command still aborts. `exit`/`fail` and fatal faults are not false returns.
Attached redirects keep DD130's operation/failure contract.

Outside a handling context, capture alone does not consume a predicate's
status: `text=$(- f)` remains the explicit stdout-only form. Conversely,
`if text=$(- f)` discards the very status it needs. `getenv` alone retains
its permission to leave lookup absence unhandled.

## Starting evidence

Source inspection at `228921e20eb5f4c6632f93b01a70ed8bd0218561` found:

- `transform/stmt_handlers.rs::try_capture_condition` declines the direct
  function value; `capture_condition_head_is_user_fn` excludes its body/
  redirected form. `reject_nonexternal_capture_condition` emits the blanket
  SB4025 refusal.
- `validate.rs::is_capture_condition_rhs` classifies the existing source
  shapes. Eligibility now needs the actual function result kind, not merely
  an external-looking head or a user-function-name exclusion.
- `transform/functions.rs::lower_inlined_body` resets condition/suppression
  at body entry. Preserve that boundary while recognizing the call's status
  consumer and completing the surrounding capture.

This is source evidence, not a new native/C execution measurement.
Implementation, diagnostic wording and internal representation are not
selected by this recording. Reuse shared result-kind/body classification
and walkers; do not create another private classification or treat predicates
as external commands to make them eligible.

## Focused acceptance

Start with a clean-output predicate so taint cannot mask status handling:

```sash
fn sample {
    echo candidate
    return 7
}
text=old
if text=$(sample) {
    echo unexpected
} else {
    int code=$?
    echo "${code}:$text"
}
```

The selected contract requires `7:candidate`, not an abort, stale `old`,
empty output or a truthy branch. This example has not been run as new
implementation evidence.

Cover true/false returns with empty and nonempty output, early returns,
forwarded statuses, and a nonzero status such as 256 to distinguish full
in-process status from process-exit truncation. An effect witness must prove
one invocation and persistent global/collection writes on false. Pin
redirect restoration and capture cleanup independently of simple acceptance.

Exercise loop and logical-list consumers, outer negation, and the existing
first-expression status-read rule. Keep integer/void, declaration,
concatenation and multi-statement refusals. A failure inside a body or nested
operand must still abort; an explicitly forwarded result may be handled.
Preserve actual handler routing rather than exempting calls from SB5101.

Use normal taint checking for the main positive cases. Separately pin a
kernel-backed result's required resolution on both outcomes, clean-buffer
behavior and shared-output ownership. A successful return must not become
a declassification operation or erase unresolved target taint.

Inventory existing direct-function refusals in `spec/capture_cond.sh` and
`spec/errors.sh` by result kind: do not retire a void/int refusal just
because it names a function. Give changed contracts fresh ids above database
history, and predict sweep totals only after the actual fixture inventory.
Prove status selection, output publication, body isolation and preservation
controls with before/reversion evidence rather than a blanket green run.

## Coordination

`reject-anonymous-capture-status-returns` keeps lexical return ownership:
the named callee may return a status, but an anonymous multi-statement body
does not thereby gain a result. `reject-discarded-status-consumers` keeps
source discard distinct from internal unchecked handling.

`recoverable-file-failures` owns direct slurp and recoverable file handling;
`bug-int-capture-redirect-switches-kind` owns integer capture's separate
channel. `bug-sb4025-advice-misdescribes-rhs` owns the broader advice repair
and its separate inner-negation/quoted-RHS questions. Preserve the accepted
capture `exec` restriction owned by `q-exec-in-capture`.
No dependency on completing those whole tasks is asserted here; resolve any
actual implementation prerequisites from their affected paths.
