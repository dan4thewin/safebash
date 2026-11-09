title: A narrow exception for next-line clauses
date: 2026-11-09

## A parser limitation was not a recorded placement decision

After conservative function names, the AI brought forward
`q-clause-on-next-line`: a brace-form `else` or `elif` could not start
the line after its preceding body. DG asked why.

The source history traced the immediate-token check to hand-parser
brace support, `217b25d1` (2026-07-04). `parse_if_brace` checks for
`Tok::Elif`/`Tok::Else` after the body and does not skip newlines.
The original commit message supplies no placement rationale.
F1+F4, `d60e74fb`, later described that behavior in the SB4072 advice;
DD68 explicitly left the placement question undecided.

That establishes the mechanism, not the original author's intent.
No historical runtime fixture was rerun for this discussion.

## A defensible rule, and a deliberate exception

DG suggested Go's rule. Go [inserts a semicolon](https://go.dev/ref/spec#Semicolons)
after a line-ending `}`, so keeping `else` on that line has a coherent
rationale. The AI had been too dismissive in calling the restriction an implementation
detail: no recorded rationale does not mean no good rationale exists.
There is no evidence here that Go was the original inspiration.

The discussion distinguished required continuation from optional
attachment. An operator or sed-chain comma leaves an operand owed
(DD94/DD96), and a header still needs its body. In contrast, the
brace-form `if` could end before an optional clause. Leading pipes,
logical connectors, redirects and `&` raise similar attachment
questions; recognizing a token as a continuation does not by itself
justify crossing a newline.

The AI consequently leaned toward announcing continuation before the
newline. DG instead chose the narrow formatting exception:

> if next-line else / elif can be had cheaply, let's allow it

An `elif` or `else` may follow the preceding brace-form body across
newlines, including blank/comment lines, without a backslash.
An intervening semicolon or statement prevents attachment. Existing
nesting and `else if` desugaring remain; a second `else` is not made
legal. No general next-line attachment for operators, redirects or
background marks follows, and no new clause-bearing construct is added.
Accepted header layouts, keyword forms and explicit continuation stay.
Capture/attest adjacency does not erase binding boundaries.

## The cost is local to parsing

At `6187dc14`, `parse_if_brace` is
`sash/src/parse/parser.rs:900-923`. The scanner already has buffered
`peek_n` (lexer.rs:443-449) and removes comments before supplying tokens
(530-537). The local route is to look past newline tokens for a clause,
consuming them only when a clause follows. Leave other successors and
their newline boundary alone; `skip_terminators` would be wrong because
it also consumes semicolons.

The existing AST and branch semantics suffice. This is source-inspected
feasibility, not a tested implementation or performance measurement.
The diagnostic must stop demanding the same line, and focused CLI
coverage must prove both the allowance and its boundaries.

## Recording and queued enforcement

DD68 carries the accepted rule; DD71 and GUIDE agree. The existing P3
`q-clause-on-next-line` is implementation work, retaining its slug and
priority rather than creating another owner. The statement-boundary
task and findings index are reconciled.

The recovery task needs an important distinction: its historical m4/r7
programs now become valid. Their acceptance cannot prove that recovery
after an actual parse error works. skw.20 needs a fresh-id contract
migration; r8's genuinely duplicate `else` and the other malformed
programs retain their recovery obligations.

No compiler, runtime, converter, spec or build code changed. No new
behavior probe, sweep or coverage run was made. Journal 577 remains
the accepted implementation baseline.
