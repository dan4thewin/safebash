title: A `for` needs something to do
date: 2026-11-09

## DG's ruling

The next question was `q-for-without-in-list`: an omitted word list
historically became an empty loop, while a semicolon before `in`
was accepted. The AI recommended requiring `in` and rejecting that
semicolon, but initially proposed retaining an explicitly empty list.

DG accepted the first two refusals and rejected that exception:

> yes to require in; reject the stray semicolon; an explicitly empty
> list does no work - pointless code should be an error, as it must
> be an unintended mistake

## Give the statement work

A word-list `for` needs `in` and at least one source item. An omitted
`in`, a semicolon before it, and a written `in` with no items all
require errors. This applies to brace and keyword bodies, including
inside other executable bodies. Marks or redirects do not make an
invalid header legal.

The explicitly empty loop is not justified by commands in its body:
none can execute. This extends the pointless-code rationale beyond
computations whose answers are ignored. It is an error, not permission
to silently remove the loop or reinterpret it as argument iteration.

The source/data distinction matters. A written list or splat may
yield no items at runtime; there is no new runtime non-emptiness
requirement. `""` is one empty-string item, not an empty list. A newline
before `in` remains legal, as does the normal separator after a
populated keyword-form list. C-style `for ((init; cond; incr))`
retains its separate grammar.

Bash's omitted-list loop means argument iteration. The converter must
spell that out as `in @args` inside functions or `in $(argv 1)` at
top level, under existing typing/taint/arity rules. This differs from
Bash's explicitly empty `in` list: conversion must not invent items
or erase the construct merely to avoid sash's error.

## Recording and queued implementation

DD68 is authoritative; DD70, DD75 and GUIDE agree. The existing P2
`q-for-without-in-list` keeps its slug and becomes implementation
work for compiler and converter. Its old keep-semicolon recommendation
and g12 empty-list preservation are superseded in place.

`reject-pointless-statements` retains its own effect/use analysis.
The explicit empty-header refusal needs no general effect inference,
and neither task depends on implementing the other. An earlier
header error must not masquerade as proof of the effect check.
Intentional `:`, constant-returning functions and ordinary consumed
results keep their existing contracts.

The task records separate refusal and preservation controls, converter
argument coverage, a fresh fixture census and before/reversion proof
obligations. No diagnostic number or implementation architecture is
selected here.

No compiler, runtime, converter, spec or build code changed. The
historical tables were not rerun and supply no new impact prediction.
No new behavior probe, sweep or coverage run was made; journal 577
remains the accepted implementation baseline.
