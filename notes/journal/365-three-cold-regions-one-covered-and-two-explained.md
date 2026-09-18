---
title: Three cold regions, one covered and two explained
date: 2026-09-18
---

# Three cold regions, one covered and two explained

After the taint fix (journal 364) the re-ranked branch queue offered
`command_is_pure` and `looks_like_dynamic_subscript_assign`. They ended
differently, which is the useful part.

## The one that was coverable

`looks_like_dynamic_subscript_assign` (`parse/parser.rs`) decides whether
a bare word `name[` begins a dynamic subscript assignment. Two of its
early-outs were cold. Line counts told me where to aim before I wrote a
thing: 20 calls reached the shape tests, and both `return false` lines
had count 0.

Both are reachable, and neither needed a contrived program:

- `a[ x ]=5` — the token after `[` is a plain word, not a `$`. The lexer
  only splits `name[` off as its own word when the subscript holds an
  expansion, so this was never a subscript assignment; `a[` is an
  ordinary command name.
- `a[$i` with nothing after it — the token where `]` belongs is the end
  of input, so the sniffer backs out rather than reading past the end.

Three specs (`80.35`–`80.37`), all rejecting cleanly with SB5011. No
hang, no ICE, no ambiguity. Sweep 5597, predicted exactly.

## The two that were not

`command_is_pure` (`transform/capture_purity.rs`) had three cold
branches, and both causes turned out to be deliberate.

The `visiting` cycle guard is already documented in the module header:
recursion cannot occur because SB5037 rejects it, and the set is carried
anyway so a malformed program cannot hang the compiler. I confirmed the
rejection end to end — direct, mutual, and a three-function cycle all
reject — which is exactly why the guard never fires. A hang guard that
never fires is working.

The `bodies.get(name)` lookup was the more interesting one: **156 calls
reach it in a full sweep and not one succeeds.** That looked like it
could be serious, because the module doc is emphatic that a body which
merely *wraps* an external (`fn emit { /bin/cat f; }`) must be
disqualified — if that walk never runs, a wrapping function could be
called pure and its output lost through the wrong capture path.

It is not. Inlining runs before this pass, so the capture arrives with
`/bin/cat` already spliced in and is disqualified by the slash test one
line above — which is where those 21 hits on the `return false` come
from. I checked the documented semantics by output rather than by
tracing the implementation: six shapes against bash — wrap, consume-and-
echo, wrap two levels deep, echo-then-external, external redirected
away, and the same function captured twice. All SAME.

One of the six first showed as a difference: the consume-and-echo case
rejected with SB5020. That is the taint rule doing its job on an
unattested `$t`; the doc's example is illustrating purity and omits the
attest. With it, SAME. Worth noting because it is the same shape of
mistake as journal 364 — a result that looks like it concerns the pass
you are studying when it comes from a different one entirely.

Both findings are now comments in the source, with the measurement
attached. That is the whole return on this one: no code changed in
`capture_purity.rs`, but the next person to see 156-calls-zero-hits does
not have to re-derive why.

## Where the cold branches actually are

Worth recording as the pattern across 364 and 365: the cold branches
that turn out to be *bugs* are the ones where the code disagrees with
its own doc comment. The ones that turn out to be fine are the ones
where the doc already explains them. Reading the comment first is a
cheap filter, provided you then check it against execution rather than
believing it.
