title: Preserve integer-function capture kind through redirects
status: open
priority: P2
kind: bug
depends: []
gates:
  - "direct integer-function captures consume the returned integer with or without attached redirects, in assignment, argument and interpolation positions"
  - "redirects still affect the function's actual I/O, without capturing or dropping stdout in place of its integer result"
  - "SB4076 rejects both $(- f) and redirected counterparts such as $(- f 2>&1) when f is an integer function"
  - "integer captures such as $(f >file) do not receive SB4075 merely for lacking a stdout-capture writer; actual stdout captures retain that guard"
  - "a void wrapper that calls - f can expose stdout through its own capture, with ordinary taint and failure checks intact"
  - "focused native/C specs fail on the pre-change binary or an independent reversion, then the ordinary predicted gate passes"

## Settled contract

DG ruled on 2026-11-06: a capture of an integer function consumes only its
returned integer (DD58/DD55, journal 583). Redirects cannot change its
result kind or turn the call into a stdout capture. Integer functions
may still write stdout; no purity check or stdout ban was chosen.

Option C remains: integer producers work in typed capture and arithmetic,
including intrinsics. Do not repair this by banning `$(f)`, interpreting an
integer as a process status, or adding new redirect forms to intrinsics.
Quoting a capture renders the integer under DD30, not the function's output.

The old F6 example is recorded in `q-nonvoid-classifier-residuals` Q1:
an f that echoes "out" and returns 7 yields integer 7 through `$(f)`, but a
redirect makes it take the statement/stdout path. `$(f 2>&1)` then draws
SB4016, while `$(- f 2>&1)` captures "out". The former must preserve integer
capture; the latter must get the same SB4076 as `$(- f)`.

A void adapter, `fn output_f { - f }`, remains valid: `$(output_f)` captures
that body's stdout after its ordinary statement discards f's integer.
Anonymous bodies retain void returns and compatible void forwarding.
Their explicit status returns are now rejected by policy (DD55/DD86,
journal 584); `reject-anonymous-capture-status-returns` owns that separate
enforcement.

## Implementation boundaries

Use the canonical function-kind and capture classification across validation,
typing, lowering and both backends. A `Redirected` wrapper must not change
the selected result channel. Keep the integer's type and value through
redirect scope/restore and caller temporaries; do not recover it by parsing
stdout or manufacture zero when setup prevented a return.

Classify integer-value versus stdout capture before applying DD88's
connected-writer check. The stdout guard must still reject genuinely
abandoned stdout captures; do not bypass it wholesale. Preserve actual
redirection order/effects and output to any enclosing active capture.

Keep no-reach, argument/body boundaries, normal taint, typed return
forwarding and integer pipeline-stage refusal. DD130's failure-class
implementation and the left-to-right evaluation work have their own owners;
this task does not invent an error-to-integer fallback or a new handling
surface. Duplicate-definition enforcement belongs to the existing P1
`reject-duplicate-function-definitions`, not new replacement machinery here.

Re-measure the current cases and affected specs before implementation.
Allocate new ids from spec.db history and predict the sweep changes;
the historical Q1 snippet is not a fresh native/C measurement.
