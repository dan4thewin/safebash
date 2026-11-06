title: Integer capture keeps its result channel
status: complete

DG ruled that a capture of an integer function only ever consumes its
returned integer. Redirects must not select stdout instead. DG noted that
an integer function writing stdout feels like a category error, but did
not make it illegal; a non-integer wrapper discarding the returned integer
is a viable, if less convenient, adapter.

The rule preserves DD58's return-type-polymorphic capture. It does not
restore the old arithmetic-only restriction, distinguish user integer
functions from integer intrinsics, or introduce purity checking.
An integer function's output follows the active output destination and
its redirects as an ordinary effect. `$(f)`, `$(f 2>&1)` and `$(f >file)`
consume the same integer. `$(- f)` and `$(- f 2>&1)` both require SB4076.

Copilot recorded the contract in DD58 and reconciled DD55 and GUIDE.
DD88's connected-writer rule now explicitly concerns stdout captures,
not every spelling of `$()`. An integer capture does not need a writer
feeding a stdout pipe. This is a consequence of DG's result-channel rule,
not a blanket exemption for actual stdout captures or new permission to
use a function capture as a condition.

The existing statement discard makes the adapter explicit:
`fn output_f { - f }` is void, and `$(output_f)` captures its stdout.
This is not a conversion of the integer. Normal taint, failure, return-kind
and no-reach rules still apply. A failed operation must not invent a zero
integer merely to complete a value-producing path.

The new P2 `bug-int-capture-redirect-switches-kind` owns shared capture-kind
classification and enforcement in both backends, including SB4075/SB4076.
The implementation must re-measure the historical F6 cases and pin the
new refusals as well as the preserved value and I/O routing. Duplicate
rejection remains in its existing P1; no new replacement machinery is
part of this fix.

Classifier residual Q1 is settled, so `q-nonvoid-classifier-residuals` is
closed as a question record. Q1/Q3 still await their separately owned fixes;
Q2/Q4 were implemented in journal 577. Anonymous capture caller-status
handling remains a separate question, not implicitly answered here.

No compiler, runtime, spec or build code changed. No full spec sweep or
coverage run was made; journal 577's accepted gate remains the implementation
baseline. Redirect-preserving capture enforcement is pending.
