title: Anonymous captures do not return statuses
status: complete

DG chose to reject explicit status returns from anonymous capture bodies,
rather than add an anonymous-result handling/discard surface. Bare return
stays legal. Copilot recorded the ruling in DD55/DD86 and reconciled DD58,
DD74, DD88 and GUIDE.

The rule is about result kind, not numeric value or the presence of the
return keyword. It rejects `return 0` just like `return 3`, and includes
status forwarding from a named function, command, test, pipeline or list.
A redirect does not change the forwarded kind. The existing void-return
contract preserves fall-through and compatible void forwarding as well as
bare return. Process termination is not a return.

The return belongs to its lexical body. A named status function called
inside a capture may still return its status, provided its call handles or
discards it. `$(- status_f)` remains the existing stdout-capture adapter.
A minus on the first inner statement is not a whole-body result discard;
no-reach remains. Direct integer captures and ordinary command/pipeline
captures are not anonymous explicit-status returns. DD88's restricted
capture-condition surface and DD101's getenv exception are unchanged.

The native CLI controls from the preceding discussion confirmed the gap:
anonymous `return 3` and `return 0` both captured `data` and continued with
process status zero. Bare return also worked. The named status-function
capture got SB4016 unless written `$(- f)`; the multi-statement condition
got SB4025. Those controls were not a C measurement or new spec coverage.

The new P2 `reject-anonymous-capture-status-returns` owns enforcement and
focused before/reversion proofs. `q-status-function-ceremony` is closed
as a question, not as implementation. Directly linked tasks now point at
the pending enforcement instead of presenting the policy as undecided.
The old eight-flip ceremony estimate is not an implementation prediction.

The next unresolved question is attached-redirect timing: retain the
stage/standalone split, evaluate arguments first in both forms, or install
the redirects first in both. Its task now lists both consistency choices.
This is separate from the already-settled left-to-right word order; no
redirect-timing ruling is recorded here.

No compiler, runtime, spec or build code changed. No new full sweep or
coverage run was made; journal 577 remains the accepted implementation
baseline. Anonymous status-return rejection is accepted but not implemented.
