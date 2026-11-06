title: Enforce distinct void, status and integer return kinds
status: complete
priority: P2
kind: bug
depends: []
gates:
  - "bare return and fall-through supply no result; explicit return 0 is status-producing in an unannotated function and integer-producing in an -> int function"
  - "status and integer functions supply a matching result on every returning path, independent of how their callers handle the result"
  - "mixed bare/valued returns and non-void fall-through reject without manufacturing zero"
  - "compatible return-producer forwarding preserves the callee's kind through redirects; self-tail calls are loop edges, not result exits"
  - "incompatible forwarding, including int-to-status and void-to-int, rejects with a type/result diagnostic rather than SB4017"
  - "focused native/C specs, before-controls and the ordinary unit/spec gate cover the new contract and preserve existing call-site handling"

## Settled contract

DG approved the return-kind model on 2026-11-06 (DD55/DD58/DD74,
journal 574). A void function uses bare returns, fall-through or compatible
void forwarding. Status and integer functions cannot use bare return or
fall off the end on a returning path. Process termination and a self-tail
back edge do not create an implicit zero return.

The numeric value does not determine the function's kind. An unannotated
function whose only result is `return 0` is a status function, not void;
its callers must handle/discard that status even though it is always zero.
An `-> int` function's explicit `return 0` supplies a real integer.
Constant-result checking is not part of this task or the language policy.

`return g` forwards g's result kind. A redirect does not turn a void result
into a status or an integer into a status. Preserve compatible void/status/
integer forwarding and reject incompatible kinds. This owns the
return-producer side of the 2026-11-05 integer/status ruling; direct pipeline
stages remain in `q-int-function-pipeline-status`.

## Implementation boundaries

Replace the current numeric/bare-zero classification with kind-based
classification and result completeness. The 2026-11-05 source inspection
found the conflation in `transform/functions.rs::returns_zero`, and
predicate-use-only completeness in `validate::check_predicate_definite_return`.
Those are starting points, not a prescribed new pass or a second classifier.
Reuse the canonical function-class and definition-in-force machinery.
Preserve operand presence separately from its rendered value: `return ""`
is not a bare return. Operand validity still belongs to the int-sink rules.

This completed unit applied the body contract to anonymous capture bodies
as required by DD86 without deciding their caller-side status consumption.
DG subsequently rejected explicit status returns from anonymous captures
(DD55/DD86, journal 584); `reject-anonymous-capture-status-returns` owns
that separate enforcement, and `q-status-function-ceremony` is closed.
This unit also preserved the then-existing redefinition behavior.
DG subsequently rejected duplicates (DD86, journal 582);
`reject-duplicate-function-definitions` owns that separate
enforcement, and classifier Q3 is no longer open. Remaining operand parsing
and enforcement of the now-settled explicit-conversion rule remain in
`bug-return-status-string-operand` (DD89, journal 579).
Do not broaden return grammar or alter EXIT/ERR's terminal protocol.

This replaces the protection that SB4017 accidentally supplied for some
missing/mismatched integer returns. `retire-return-ceremony-check` depends
on these guards; the two tasks may land in one coherent implementation unit.

## Focused acceptance cases

- Void early return and fall-through remain legal; their ordinary calls
  have no result to discard.
- Status-zero functions are non-void and valid predicates; bare calls are
  SB4016, not silently admitted as void.
- Bare returns and fall-through in non-void functions reject even when
  the call is discarded rather than used as a predicate.
- Compatible forwarding works, including redirected void forwarding and
  self-tail control flow; incompatible forwarding rejects.

Implemented and verified in journal 577. Shared body lowering checks named
and anonymous bodies with the canonical definition-in-force classifier.
SB4037 rejects missing results; SB4030 rejects incompatible forwarding.
The final ordinary gate passes 11345 checks (11332 passing, 13 skipped).
Explicit disposition conversion was subsequently settled in journal 579;
its implementation remains separate. Anonymous-capture status-return
rejection is also accepted but awaits its separately owned enforcement.
