title: Integer values are not status results
status: complete

After recording DD129, the AI presented B79: an int function used as a
pipeline stage returns 3 as failure but 256 as success, because its value
becomes an eight-bit process status. These were the existing closeout
observations, not new probes.

The AI recommended rejecting bare int stages while permitting `- f` as an
explicit stage discard. DG rejected that distinction:

> in my mental model, a status and int result are essentially different types;
> treating an int function's returned value as a status should be the same
> family of error as illegal mixing of string and int. thus, suppression is
> not appropriate

The contract is typing, not error handling. A direct int stage is SB4030
with or without `-`, in every pipeline position and handling context.
Suppressing a status cannot make the source call well-typed.

Applying the principle also removes DD55's explicit allowance for an
unannotated/status function to use `return int_fn` and reinterpret the
integer as a status. That contrary clause was rewritten in place, rather
than leaving a later amendment to contradict it. Int-to-int forwarding,
ordinary captures and in-process integer-value discard remain valid;
explicit numeric status construction and status predicates are unchanged.
A wrapper must discard a value or deliberately produce a status.

The ruling is recorded in DD58, with matching DD55/DD74 and GUIDE text.
The existing `q-int-function-pipeline-status` slug is retained for links,
but its kind is now `bug`: the choice is settled and enforcement remains
open. B79 stays unchecked in the findings index. The adjacent classifier
questions retain their separate scope.

This is documentation only. No compiler, runtime, dependency, spec or build
rule changed, and the return-producer consequence was not newly probed.
Neither this typing repair nor the DD129 runtime repair has started.
Coverage remains paused.
