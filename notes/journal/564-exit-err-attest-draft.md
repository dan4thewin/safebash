# 564 - Drafting EXIT, ERR and adjacent taint resolution

2026-11-03. After W4, DG asked for the W5 issues to be explained, then
developed a different cleanup model in discussion. DG requested a draft DD;
I wrote DD128, initially leaving five choices explicitly open. DG's later
rulings are recorded below. The document remains a draft, not an
implementation.

The proposed split is cause-based. Normal completion, explicit zero exit
and configured signal shutdown enter ordinary checked EXIT. An uncaught
operational failure or explicit nonzero exit enters ERR. ERR uses a
conservative incoming-taint contract and requires explicit handling of its
own operational failures. A failure in EXIT can enter ERR, but neither
handler re-enters itself or returns to the interrupted program.

DG tightened adjacency beyond the current DD121 assignment kill: an
unrelated clean overwrite must not clear outstanding taint. Resolution
belongs to the adjacent attest and its failed-branch cleanup. This removes
the exception I had suggested for overwriting a value inside EXIT before
reading it. The draft spells out consequences for unset, collections,
loop rebinding, helpers and slots, as well as the distinction between an
acquisition-failure fallback and an attest-failure fallback.

DG also identified the limit on recovery. I had initially suggested that a
literal no-abort rule would exclude even operations that could exhaust the
heap. The draft instead keeps arithmetic faults and heap exhaustion fatal,
without pretending that suppression catches them or banning ordinary
arithmetic and allocation from ERR. Other unhandled errors do not become
fatal merely because they lack a handling form.

DD89 settles the status representation: full i64 internally, low byte only
at the process boundary. It does not settle the new handler selection:
one could inspect the low byte without discarding the full value. The
initial draft used the full value, including ERR selection for `exit 256`,
and marked this choice for review. Four further choices were END
compatibility, whether a first fatal fault attempts ERR, lexical versus
summary-proven helper fallback, and fresh input acquired inside ERR.

Accepted DDs remain unchanged. The draft identifies their eventual coherent
rewrites, the W5 evidence and the wider implementation requirements,
including the existing handler type-check gap. No compiler, runtime or
spec changes, implementation authorization, new probes or spec-history
writes are part of this task.

## DG settled the five choices

Later on 2026-11-03, DG ruled:

1. Reject END.
2. Attempt ERR once after a first fatal fault.
3. Write failed-attest replacement directly in the branch.
4. Allow attestation of fresh input acquired inside ERR.
5. Explicit exit calls route 0 to EXIT and nonzero to ERR; signal and
   fall-through route to EXIT.

I incorporated these into the DD's rules and replaced the alternatives
table with the rulings. The fatal-fault choice reverses the initial draft's
direct-termination default: a first main-process fault, including in EXIT,
attempts ERR; a fault inside ERR terminates without re-entry. Entry itself
must not need a fresh heap allocation, and the failed operation is not
resumed. Child faults still do not execute parent handlers.

The exit rule uses the full integer, so `exit 256` selects ERR while a
signal's nonzero saved status does not. A helper's proven clean write is
not a substitute for lexical failed-branch replacement. Fresh ERR input
can be attested and ordinarily read; incoming potentially tainted data
still requires the conservative escape policy.

These are settled choices within the draft. No implementation has started,
and the companion DD rewrites remain part of finalizing the design.
