title: DD#136 - ERR-owned call attempts
status: accepted

1. ERR remains a checked executable body. An unhandled failure without an
   eligible recovery boundary terminates ERR; it neither re-enters ERR nor
   resumes the interrupted program. The former SB5101 prohibition on possible
   operational aborts is replaced, not a license to disable their checks.
2. In ERR's own body, `: FUNCTION ARG...` attempts one named void-function
   call. Normal completion or a recoverable abort continues after the call.
   The attempt has no result and cannot be a condition, stage or background
   job. Unknown functions, wrong return kinds and other contexts are errors.
3. A caught status-function call in ERR establishes the corresponding
   attempt. Normal completion supplies its ordinary status; a recoverable
   abort takes the failure route. Existing legal consumers and negation
   retain their meanings. Source `-` remains ordinary result discard and
   does not establish an attempt.
4. Only ERR-owned call sites establish recovery boundaries. A helper's
   condition is ordinary local result handling, not another catcher.
   Recovery abandons the entire intervening call chain. Integer functions
   cannot establish attempts, but may execute within one; no missing
   integer or unfinished captured value is replaced with invented data.
5. The attempted call includes argument evaluation and attached redirects,
   with the ordinary evaluation order. Its boundary is inactive before
   either continuation begins.
6. Recovery has a narrow boundary: explicit `fail [N]` and
   existing unchecked operational results can abandon an attempt.
   Locally handled results never raise that abort. Existing unconditional
   runtime guards remain terminal, including `:?`, argv-NUL and invalid-index
   guards, and currently unconditional slurp/read failures.
7. Fatal faults and explicit `exit` bypass attempt recovery. Failed
   restoration terminates cleanup rather than continuing with uncertain
   execution state. Fork failure remains fatal under DD130.
8. Abort cause and numeric status remain distinct. `fail 0` is an abnormal
   failure, not a successful status return. A caught abort retains its full
   status before authored negation. Negation operates on the attempted
   invocation's verdict and makes its ordinary numeric 0/1 update.
   A resultless colon attempt grants no new status-read permission.
9. Completed application effects remain. Restore only the abandoned
    attempt's redirects, captures and readers before continuation; preserve
    enclosing ERR resources. Close abandoned readers before waiting for
    their producers, without killing those producers.
10. Taint analysis includes the states at recoverable abort sites, not only
    normal function returns. An attempt cannot borrow a later assignment,
    attest or fresh-lifetime initialization. Existing readonly, ownership,
    adjacency, forced-read and process-boundary rules remain.
11. Handled failures have no automatic abort diagnostic, as in DD70.
    Authored output and external stderr are unaffected. A terminal failure
    retains its normal diagnostic. ERR fall-through retains its original
    entry status, independently of individual attempt outcomes.
12. Bare `:` remains the successful noop, including redirect-only uses.
    Colon with operands no longer evaluates and ignores arbitrary data.
    Use the existing write-only `_` binder for value disposal: `_=$value`.
    Disposal preserves RHS effects and checks and establishes no recovery.
    Separate sinks may have different types; none makes `_` readable or
    permits a compound update that would read its discarded previous value.

---

DG accepted the complete implemented branch on 2026-11-26:

> take the current branch to main as accepted (no longer provisional)

[Journal 676](../journal/676-accept-err-attempts.md) records acceptance and
the main landing. The original provisional implementation was requested on
2026-11-21, including DD alignment, the tour and the specification outline.
Its retained branch is `err-attempt-trial`, based on `02217248`.
The [integrated owner](../tasks/err-call-attempt-trial.md) records delivery
and the companion-rule inventory.
[Journal 664](../journal/664-err-owned-call-attempts.md) records the
implementation, coupled repairs and verification. The separate evaluation
boundary is now satisfied; the integrated owner is complete.

## The recovery point belongs to ERR

DG corrected the initial nearest-function interpretation:

> an abort/fail needs to abandon the call stack, resuming within ERR in
> something like an else branch.

He then proposed limiting the region to one user-function call, with
different consumers for void and status functions. A failure in
`ERR -> archive_report -> copy_report` skips the remainder of both helpers.
It does not return normally from `copy_report` and let `archive_report`
continue deleting its input.

The final spelling separates three operations:

```sash
:                       # successful noop
_=$value                # evaluate and discard a value
: archive_report        # ERR-only void-function attempt
```

For a status function, the ordinary caught form expresses the recovery
branch:

```sash
if recover_report {
    - echo recovered
} else {
    - echo "recovery failed: $?"
}
```

This extra reach belongs only to the ERR-owned call boundary. An `if`
inside `recover_report` still handles its operand's ordinary result, not
aborts inside a called helper. A locally caught command failure never
becomes an attempt abort in the first place.

`- status_function` keeps its existing meaning: discard the function's
normal result. Use a caught call, such as `status_function || :`, when
ERR should also survive a recoverable body abort. A bare void call likewise
does not silently become best-effort.

The call boundary is removed before either its success or failure
continuation. A failure in the continuation cannot jump back into itself.
That continuation may establish another attempt.

## The deliberately narrow boundary

DG selected **"narrow"** after comparing two interpretations:

- Recover explicit `fail` and existing unchecked operational results,
  retaining terminal behavior for unconditional runtime guards.
- Extend recovery to every nonfatal failure, including value-producing
  operations and guards without an existing result channel.

The second interpretation is not part of this decision. In particular,
wrapping `${value:?message}` in an attempted helper does not make it a
recoverable result, and an argv-NUL guard still ends ERR. The same applies
to a slurp whose current interface unconditionally aborts on open/read
failure. No value is fabricated in either case.

This is a contextual recovery boundary, **not a reclassification of ordinary
file errors as globally fatal**. DD130 and the separately owned
[failure-classification](../tasks/runtime-failure-classification.md) and
[file-handling](../tasks/recoverable-file-failures.md) work remain valid.
Their future changes must explicitly account for this recovery inventory.
Code generation must not infer recoverability merely from an exit status,
or redirect every call of `sash_abort` into recovery.

Existing operational checks include commands, pipeline outcomes, checked
redirect opens, standalone attests and implicitly checked `mkdtemp` results.
Their actual cause still matters: in particular, a pipeline's failed fork
cannot become a recovered command result. Runtime-only guards, arithmetic,
heap exhaustion, restoration faults and source `exit` do not use the
attempt continuation.

## State after an abandoned call

The failed path exposes the completed prefix, not a transaction or a normal
return. A file may already have been created, output emitted or input
consumed. A handler-owned global may have been updated before failure.
Neither runtime cleanup nor compiler analysis undoes those effects.

An attest that the failed path did not reach supplies no proof. An attest
that failed does not clean its subject. A fresh acquisition that the path
never reached cannot refresh inherited taint or authorize its attestation.
DD127/DD128 still govern ownership and adjacent resolution; DD135 still
makes actual ERR imports readonly.

Abandoning a call must restore its scoped machinery without dismantling an
outer ERR reader or capture. Parent recovery destinations never enter a
forked child. Child outcomes remain process statuses under the existing
pipeline/job rules, not jumps into the parent's generated code.

## Implemented mechanism

Use compiler-visible exceptional control-flow edges. User functions are
already inlined, so abandoning their language-level call chain does not
require a machine-stack jump. Keep the checks and give the eligible failure
branches an ERR-owned destination.

The same edges must be visible to cleanup planning and taint analysis.
Runtime-only nonreturning guards retain terminal dispatch. This does not
introduce general `setjmp`/`longjmp` through Rust frames, and does not change
DD128's actual-global storage or generated handler callbacks.

Both native and C backends implement these rules. Journal 664 records real
CLI before/reversion controls, cleanup and taint evidence, and the complete
ordinary gate; journal 676 records the accepted main landing.
