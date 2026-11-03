# 564 - Drafting EXIT, ERR and adjacent taint resolution

2026-11-03. After W4, DG asked for the W5 issues to be explained, then
developed a different cleanup model in discussion. DG requested a draft DD;
I wrote DD128. It is explicitly a draft, not an accepted decision or an
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
draft uses the full value, including ERR selection for `exit 256`, and
marks this choice for review. Four further choices are END compatibility,
whether a first fatal fault attempts ERR, lexical versus summary-proven
helper fallback, and fresh input acquired inside ERR.

Accepted DDs remain unchanged. The draft identifies their eventual coherent
rewrites, the W5 evidence and the wider implementation requirements,
including the existing handler type-check gap. No compiler, runtime or
spec changes, implementation authorization, new probes or spec-history
writes are part of this task.
