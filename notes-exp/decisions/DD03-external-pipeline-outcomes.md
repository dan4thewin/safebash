---
title: "DD#3 - External pipeline outcomes and checked completion"
status: accepted
---

DG selected the completion policy for the next multi-stage milestone:

> Retain every stage outcome; checked pipelines fail on any stage; use the
> rightmost failure for numeric status.

The alternatives presented were last-stage-only status, which can hide an
upstream failure, and exposing individual results without selecting an
aggregate yet. The accepted policy preserves each stage's normal exit,
signal, or setup error. Only a fully successful pipeline succeeds.

Checking a completed pipeline raises the rightmost nonzero exit or signal;
signal-to-128-plus-number conversion occurs only at this checked boundary.
Earlier failures remain inspectable. Exit numbers are not treated as a
severity ranking. This is an external-target choice, not a change to the
native compiler's aggregate.

The first construction/setup failure remains distinct and takes precedence
over process verdicts caused by its cleanup. Stages that were not started
have no invented success or exit status. Failed setup skips the consumer,
closes all parent pipe endpoints, and accounts for children already started.
Cleanup does not force-kill or impose a runtime timeout.

Setup and cleanup errors are recorded separately from known stage outcomes,
in encounter order. They cannot overwrite a child's exit or signal; checking
raises the first recorded setup error. A failed pre-exec setup remains the
stage's effective outcome rather than becoming ordinary exit 127. If a wait
cannot supply any child status, that stage retains the wait error instead.

The initial implementation is multiple external stages followed by an
explicit parent reader. It reuses DD#2's single-threaded launch path and
DD#122's close-before-wait ownership rule. Consumer exceptions remain their
own errors, and consumed state is not rolled back on producer failure.

Stage/function suppression, negation, forked C++ stages, public wait, END,
and signals beyond existing fork/exec inheritance are outside this slice.
