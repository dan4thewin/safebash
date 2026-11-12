title: Reject explicit status returns from anonymous capture bodies
status: open
priority: P2
kind: work
depends: []
gates:
  - "anonymous capture bodies reject explicit status returns, including zero, handling-position return $? and status-producing command/test/pipeline/list or function forwarding"
  - "the rejection follows the return's owning body through nested control flow and captures, in assignment, argument and interpolation positions and through redirects"
  - "bare return, fall-through and compatible void forwarding remain legal, including redirected void forwarding"
  - "named status functions retain their returns and explicit call-site handling/discard, including when called inside a capture"
  - "DD88 named-predicate capture conditions consume the named callee's status, not an anonymous-body return; coordinate with predicate-capture-conditions"
  - "direct integer captures, ordinary command/pipeline captures and existing DD88/DD101 capture-conditions retain their contracts"
  - "a located compile-time diagnostic explains the anonymous-body restriction without suggesting unsupported discard or condition syntax"
  - "focused native/C specs have meaningful before/reversion controls and the ordinary predicted gate passes"

## Settled contract

DG chose on 2026-11-06 to reject explicit status returns from anonymous
capture bodies rather than add a new result-handling surface
(DD55/DD86, journal 584). `return 0` is a real status return, not void;
this is not a constant-result check.

The rule includes status forwarding through `return status_f`, external
commands, tests, pipelines and lists, with or without redirects. Follow
the existing result-kind classification, not just operand presence:
`return void_f` supplies no result and remains legal. Bare return and
fall-through remain legal too. Process termination is not a return.

A body that needs to return a status must be named. Its call sites use
ordinary explicit handling/discard; `$(- status_f)` captures stdout and
discards the returned status. DD88 also accepts `if text=$(status_f)` and
the other existing capture-condition consumers (2026-11-12); the separate
`predicate-capture-conditions` task owns that extension. Do not prohibit
status returns in a named callee merely because its caller is a capture.

## Evidence and implementation boundaries

The original F6 filing is retained in `q-status-function-ceremony`.
Native controls on 2026-11-06 confirmed that
`str text=$(echo data; return 3)` and the `return 0` variant both capture
`data` and continue with process status zero. The bare-return control also
works. A named status-function capture gets SB4016, its `$(- f)` form
works, and the multi-statement capture-condition control gets SB4025.
These are native observations, not a new C measurement or spec coverage.

Reuse shared return-kind and body-boundary machinery. Check the return's
owning anonymous body, including nested control flow and nested captures;
do not walk through a named call and ban the callee's legitimate returns.
Do not silently drop a status, turn it into an integer/stdout result or
replace the rejection with an implicit runtime failure check.

Preserve no-reach: a minus on the first inner statement does not consume
the whole body's return. Do not introduce anonymous-body result handling or
general multi-statement conditions. DD88's named-predicate extension must
not inherit condition or suppression context into the callee's body. Ordinary
external-command and pipeline captures still have their existing status
checks; this rule does not ban those captures. DD101's getenv exception
and DD58's direct integer captures remain.

Preserve scope, taint, return completeness and typed forwarding.
Duplicate-definition enforcement, integer-capture redirect repair and
explicit string-to-integer return conversion remain separately owned.
No diagnostic code or sweep delta is chosen before the implementation
inventory. Re-measure affected cases, allocate ids above spec.db history,
and distinguish new rejection proofs from existing malformed-return
diagnostics that could mask this rule.
