title: Arguments before attached redirects
status: complete

DG chose **"go with arguments first"** on 2026-11-07. A standalone
command and a pipeline stage evaluate and retain their arguments before
processing their attached redirects. Copilot recorded the rule in DD86,
with DD55/DD122/DD130 and GUIDE describing its process and failure
boundaries.

This completes the timing choice left separate by journal 580's
left-to-right ruling. The order within words and across arguments remains
left-to-right. After arguments, each redirect target is evaluated and its
redirect applied in source order; a failure stops later redirects and the
command body. Target reads see completed argument effects, and earlier
argument values are not loaded again after later work. DD119's target
restrictions remain unchanged.

The comparison that made the old split visible used an integer helper:

```sash
fn number -> int {
    echo "helper ran" >&2
    return 7
}

/bin/echo "$(number)" 2>standalone.err
/bin/echo "$(number)" 2>pipeline.err | /bin/cat
```

During the preceding discussion, the ordinary native CLI at `75984b2e`
compiled this without a taint bypass. It printed two lines containing 7,
but only one helper message reached inherited stderr: `standalone.err`
was empty and `pipeline.err` contained the other message. Under the new
rule both helper messages reach inherited stderr and both files remain
empty. The redirects attach to the outer commands, not to `$(number)`,
so this does not depend on the separate pending integer-capture redirect
repair.

The Bash comparison used `echo 7` rather than `return 7`, since Bash
captures stdout. It printed the same two values with both helper messages
on inherited stderr and both files empty. That was a comparison, not a
byte-compatibility requirement. The Sash and Bash observations preceded
the ruling; no new probe or C measurement was made while recording it.

Pipeline plumbing still precedes a stage's argument evaluation, and the
arguments still execute in its child. Moving them into the parent would
restore B81's bug, not implement this choice. A capture retains its own
output channel and inner redirect scopes; already-active enclosing
redirects remain active. DD122's parent-process readers and lexical pipe
ownership do not change.

Failure ordering follows from the phases. An argument failure keeps its
own checks and no-reach boundary and occurs before attached redirects.
A redirect failure cannot undo completed argument effects, any more than
it rolls back an earlier file truncation. DD130 still owns failure
classification, restoration and recoverable continuation; this recording
does not implement that work.

`q-stage-capture-redirect-order` keeps its existing slug and P3 priority,
but is now an open implementation bug, not an unanswered question. Its
gates cover both backends, target reads, routing, failure order and
stage-child ownership. stgarg.33-35 need changed contracts at fresh ids;
stgarg.36 remains a standalone preservation control. Those historical
specs were not re-run for this recording. The separate hoisting and file
failure tasks, and B81's findings entry, now point at the accepted rule.

No compiler, runtime, spec or build code changed. No full sweep or coverage
run was made; journal 577 remains the accepted implementation baseline.
Arguments-first timing is accepted but not yet enforced in stages.
