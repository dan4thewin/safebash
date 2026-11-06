title: Explicit arithmetic at every integer sink
status: complete

DG chose "require explicit" for string-valued dispositions at return
operands. `return ${s:-7}` and `return ${s:?bad}` must not implicitly enter
arithmetic merely because the destination is numeric. The accountable
spellings are `return $(( ${s:-7} ))` and
`return $(( ${s:?bad} ))`, for status and integer results alike.

This keeps return consistent with the other integer sinks. It does not
loosen their conversion rules, retain a return-only exception, change
string-context dispositions, or turn an already-integer disposition into
a string. The existing integer `:?` classification bug must be repaired
before the return guard is strengthened, or in the same coherent unit.

Copilot recorded the ruling in DD89/DD74 and GUIDE, removed DD89's false
claim that SB4030 recommends a bare return disposition, and updated
`bug-return-status-string-operand` with an explicit dependency on
`bug-expr-type-int-disposition`. Both implementation tasks remain open.
Old conversion assertions must survive under explicit arithmetic spellings,
with fresh IDs and separate refusal cases for the bare string-valued forms.

The next decision presented is capture evaluation order:
`q-capture-hoisting-order`. Its global-mutation example was rechecked with
the current ordinary native compiler at `51af4312` and still prints
`new X new`, rather than the `old X new` that left-to-right word evaluation
would produce. No evaluation-order ruling has been made.

This records policy and a fresh probe only. No compiler, runtime, spec or
build source changed; journal 577's accepted gate remains the baseline.
