title: Visible boundaries, local resolution
date: 2026-11-09

## The asymmetry was two grammars

The next question was `q-statement-form-residuals`. DD71 explicitly
permitted `a=foo bar` to mean a binding followed by a command; the
task's description of that permission as unrecorded was incomplete.
DD53 never made it a per-command environment assignment.

DG initially rejected the implicit split, then asked for a fuller
discussion. Two plain string bindings could precede a command, while
adding `str` or `int` stopped that from working. DG found the asymmetry
undesirable, but did not want a semicolon where a construct already
had an obvious closing boundary.

The AI read `sash/src/parse/parser.rs` through the index, rather than
rerunning the old fixtures. At `b6c26a9a`, `parse_assignment` returned one
Assignment after its RHS (2620-2678); the outer loop accepted
zero terminators before starting another statement (125-139,229-233).
A declaration instead contained a list of declarators
(1157-1173,1220-1229), including bare names (1329-1330). The parser
therefore saw three statements without the keyword and one greedy
declaration with it. Command-name lookup would not be a sound boundary:
a declarator may have the same spelling as a command.

## DG's ruling

The discussion separated explicit statement endings from operand
closers, and proposed uniform binding lists rather than requiring a
semicolon between every plain assignment. It also raised the semantic
question hidden by grouping: what counts as adjacent to an attest.

DG chose:

> yes to no extra separators when there are clear boundaries and yes to ;
> after >=1 assignment. a capture followed by an attest should be a unit
> rather than multiple catures in one statement followed by multiple attests

A completed brace/keyword construct or other self-delimiting statement
needs no additional separator. Its legal continuations still attach;
a raw closing character inside an operand is not a boundary. An
enclosing closer or EOF ends the final statement too.

One or several bindings use the same rule, with or without a declaration
keyword: separate them from a following statement with `;` or newline.
`a=one b=two; echo "$a $b"` and its declared counterpart are valid.
Bindings execute in order, not as environment prefixes, simultaneous
assignments or a transaction. Existing per-binding constraints remain.

## Acquisition and resolution stay together

The binding list does not become an attest batch. Each binding is an
adjacency step, so a later binding intervenes even when it is clean.
The last binding may have an adjacent attest; an earlier acquisition
cannot borrow that position. Write one capture/resolution pair before
the next, rather than several captures followed by several attests.

This preserves the existing distinction between tainted acquisition
and clean production. It introduces no mandatory attest for every
capture, removes no explicit `:!` escape, and does not split a
whole-collection attest-all into per-cell source operations.
DD128 still protects each legal resolution pair against signal
dispatch; grouping cannot extend that protection into a batch.

## Recording and queued implementation

DD71 and DD121 carry the grammar and locality rules. DD53, DD68,
DD128 and GUIDE agree; the old DD71 split permission is rewritten in
place rather than left beside a contradictory amendment.
The completed operand-word task is marked historical on that one
point, without reopening its unchanged lexer work.

The existing P4 `q-statement-form-residuals` keeps its slug and becomes
implementation work. Its proof obligations distinguish whole-statement
and operand endings, one/many typed/untyped bindings, ordered effects,
and per-binding attest locality. Group retirement and identifier
validation keep their independent owners; an earlier refusal by one
of those guards is not evidence that the separator guard works.

No new diagnostic number or internal representation is selected.
No compiler, runtime, converter, spec or build code changed. Historical
tables were not rerun, and no new behavior probe, sweep or coverage
run was made. Journal 577 remains the accepted implementation baseline.
