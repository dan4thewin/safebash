title: Conservative function names
date: 2026-11-09

## A path was also a function name

After settling statement boundaries, DG asked for the next decision.
The AI brought forward `bug-function-name-shadows-command-path`, which
also owns the function-name question left by declaration validation.

Its October probes accepted names such as `a-b`, `9`, `a[0]` and
`/bin/true`. The last was more than unusual spelling: checks inspected
the external program while lowering called the function. A numeric
name also competed with `return`'s value/producer classification,
whose choice depended on whether the numeral fit in i64.

Those are historical observations, not new measurements. DD125 already
reserved compiler-owned names, DD23 gave paths their external meaning,
and DD86 now requires one definition per function name. None supplied
the general function-name spelling rule.

## DG's choice

The AI recommended the variable identifier grammar, with existing
reserved-name exclusions. The alternatives were the task's earlier
collision-only ban, preserving punctuation such as hyphens and dots,
or arbitrary names with consistent function-first lookup.

DG answered:

> yes to conservative names

The accepted grammar is:

```text
[A-Za-z_][A-Za-z0-9_]*
```

It applies to all source definition spellings and all return kinds.
An invalid name is rejected at its definition, even if never called.
`read_file`, `_helper2` and `__helper` satisfy the spelling rule;
paths, numerals, digit-led names, punctuation and non-ASCII names do
not. Existing reserved-name and collision rules still apply. The
compiler must avoid source `__` names, not reserve them.

The cost was stated in the proposal: Bash names such as `read-file`
need valid replacement names. This is not permission to normalize
punctuation blindly or merge two distinct functions into `read_file`.
Converter impact and a safe migration mechanism remain to be assessed;
DD57 already provides the annotated manual route when a mechanical
conversion cannot preserve meaning.

## Naming is not return typing

The numeric-name fixtures now require definition-site refusals.
That must not masquerade as proof that an out-of-range return literal
is checked correctly. The return-operand task needs separate legal-name
fixtures, and the variable declaration/for guard needs its own evidence
too. Sharing an identifier predicate does not make one guard prove all
three.

External filenames, arguments and map keys are unaffected. Legal
external return forwarding remains legal; no new classification of
every non-identifier command head was inferred from a function-name rule.
Scope, hoisting, return kinds and duplicate-definition policy stay intact.

## Recording and queued enforcement

DD125 carries the grammar, with DD23/DD86 and GUIDE pointing to it.
The existing P2 naming bug remains open for implementation. Its old
unanswered question and narrower recommendation are replaced in place;
the observations remain explicitly historical. The former clean-function
taint assumption is not carried forward as a preservation obligation:
DD127/DD128 now govern those controls.

The declaration-name and return-operand tasks, findings index and status
are reconciled. Implementation must measure affected specs/converter
output, distinguish naming refusals from unrelated diagnostics, and prove
the naming guard with before/reversion controls before the normal gate.
No diagnostic number or parser/validator architecture is selected here.

No compiler, runtime, converter, spec or build code changed. No historical
fixture was rerun, and no new behavior probe, sweep or coverage run was
made. Journal 577 remains the accepted implementation baseline.
