# 633 - Indexed updates evaluate their key once

DG's tour polish exposed an indexed-update bug. Removing unnecessary braces
from the map example was straightforward, but its proposed `++` would change
DD79: bare arithmetic cell mutation is for integer arrays, not maps. The
tour now uses the existing keyed `seen[$w]+=1` form. No map-mutation extension
was selected.

The investigation found that arithmetic `a[index] op= rhs` was expanded by
the parser to `a[index] = a[index] op rhs`, copying an effectful index.
Ordinary indexed `+=` independently lowered its read and write indices too.
Both backends therefore called a key-producing function twice. There was a
second, coupled error: an RHS could change the selected key or old cell value
before the final store used them.

DD86 already requires earlier values to survive later effects. The AI
implemented that existing rule rather than choosing a new evaluation order.

## Repair

Arithmetic cell updates retain their operator in `AssignOp::Arith`, one index
and the original RHS. Shared indexed-write lowering materializes the typed
key first; compound updates also materialize the old cell value before the
RHS runs. Plain `=` saves only the key. Integer addition and string
concatenation for ordinary `+=` use the same path.

The snapshots use `reserve_temp_name` with their actual types. Reusing an
SSA variable reference alone is insufficient: native and C storage collapse
versions of one binding, so a later write can change what that reference
reads. The lowering reuses the canonical cell builder, including collection
slot resolution and checked key/element types; arithmetic reads now share
that builder too.

The retained operation still represents an implicit target read. Scope/use
accounting and taint analysis preserve that obligation. A direct pass test
checks the latter independently: typing already rejects string-array
arithmetic, so an end-to-end refusal cannot prove this taint check.

The inert IR advances to schema 3. Arithmetic cell assignments carry their
operation rather than a synthetic RHS cell read. DD104 records the shape and
retains schema 2's separate EXIT/ERR fields. Five changed IR contracts take
fresh `ircell.1`-`.5` IDs, including their C-mode skip counterparts; scalar
mutation and C-for desugaring contracts are unchanged.

No backend or runtime change was needed. This is a bounded indexed-write
repair, not completion of the general
[expression-order work](../tasks/q-capture-hoisting-order.md).

## Evidence

`subonce.1`-`.12` cover arithmetic compounds, postfix updates, ordinary
integer/string updates, RHS changes to the key and old cell, and preserved
typing/read/taint checks. Against the frozen original compiler, `.1`-`.9`
fail in both modes and `.10`-`.12` pass; all 141 preceding subscript cases
still pass. The positive compiler passes all 153 in native and C.

The map diagnostic case counts the duplicated `is a map` message, not every
SB4030: the independent key-kind error remains. The tainted-key case pins
the index read, not the implicit old-value read.

Six independent reversions distinguish the repaired mechanisms:

| Reversion | Failure witnesses |
|---|---|
| Restore parser-created RHS cell read | `subonce.1`, `.2`, `.3`, `.6`, `.9` in both modes; `ircell.3` in native |
| Stop saving the selected key | `subonce.6`, `.7`, `.8` in both modes |
| Stop saving the old cell value | `subonce.6`, `.8` in both modes |
| Re-evaluate the ordinary `+=` read key | `subonce.4`, `.5` in both modes |
| Omit the implicit semantic read | `subonce.11` in both modes |
| Omit the implicit taint read | The direct pass test loses SB5021 |

Every predicted set matched exactly. All stubs were restored; source/spec
hashes and the release compiler matched the frozen positive before the gate.
The original compiler also fails all seven non-skipped replacement IR
assertions; the three native-only IR cases retain their C skips.

Targeted subscript, arithmetic, miscellaneous and CLI files pass in both
modes. Formatting, Clippy including tests, totality and the live
grandfather-on-modify complexity rubric pass without bypass.

The serialized gate passes with:

```text
NEW=34 RETIRED=10 FLIPS=0 TOTAL=11369 timeout 900 mk -p 1 gate
```

Its sweep is rows **6902132-6913500**: **11356 passing, 13 existing skips,
zero failing**, native 5796 and C 5573. All 11335 retained prior IDs have
unchanged results; the exact 34 additions and ten retirements match the
prediction. Liveness is 251. `mk test` passes 239 compiler, 27 converter and
20 expansion tests. The tour checker reports 49 snippets with no mismatch.

Private evidence is under the session's
`files/tour-subscript-once-20261113/`: frozen compilers, before/positive
outputs, reversion logs and TSVs, source identities, and the gate comparison.

## Boundary

The repair and tour remain uncommitted on `spec-outline`, alongside DG's
outline and policy review drafts; HEAD remains `2ef86c2a`. The gate's commit
label therefore identifies the unchanged HEAD, not a published fix.

DD134's element-typed loop binders and the symmetric lookup policy remain
queued, not implemented here. The `write` discussion remains unsettled.
Both local lockfiles and the runtime archive are unchanged. No coverage run
or unrelated implementation was started.

## Tour follow-up: justified trusted reads

DG corrected slide 47's framing of `:!` as wanting the input "as-is".
Its purpose is to omit a redundant attest when ordinary code already
validates rigorously, or when the intended use is safe for any contents.
The slide now demonstrates `strlen $input:!` without redundant braces and
describes exhaustive dispatch over `sash-getopt` output as the other case.
It preserves the per-read boundary: no validation or binding-wide
declassification is implied. This clarifies DD91, not a new taint rule.

## Tour follow-up: canonical names beside familiar shorthand

DG requested keeping familiar `$1`, `$2` and `$#` in the function snippets,
while naming their canonical `$args[0]`, `$args[1]` and `$(len args)` forms.
The variadic slide distinguishes whole-array use from counting: the latter
does not make a function variadic (DD81).

The failure and signal snippets now use `$(status)` and `$(getpid)`, with
`$?` and `$$` mentioned as aliases. The background example already used
`$(getcpid)`; its prose now also names `$!` and distinguishes `waitstatus`
from `status`. These are DD75/DD126's existing forms and availability rules,
not new language behavior. The tour and all other review work remain
uncommitted.

All 49 snippets still pass. Exact-output checks also confirm the variadic,
failure-handling, background-job and signal examples, so the signal example
cannot pass merely by failing to compile.

## Tour follow-up: unary shape checks before ordinary code

DG preferred the useful `p?`/`g?` shorthands to regex-heavy attests, and
asked to show a compound predicate allowing empty or printable input.
The introductory, caught-failure, collection and captured-output examples
now use those unary forms. Their explanation preserves DD93's exact
boundary: both require non-empty ASCII text; `p?` permits spaces and `g?`
does not. Attestation establishes that shape, not application-specific
meaning. The passwd example first attests printability, then checks the
root UID with an ordinary `if` before making its claim.

The new compound-attest slide uses `attest s [[ -z $s || p? $s ]]`, followed
by `echo ${s:-empty}`. Its `$(arg 1 "")` explicitly resolves an absent
argument without introducing a separate lookup-status lesson; a supplied
empty argument remains empty. The caught-failure example uses the missing
third argument, so the tour actually exercises the failed-check branch.

All 50 snippets pass. Exact outputs pin the edited examples; eleven direct
runs of their extracted source cover missing/empty arguments, printable
spaces, graphical punctuation, control bytes, non-ASCII input and the
caught check's successful branch. Logs are retained as
`tour-unary-attests-check.log` and `tour-unary-attests-results.json` in the
same private evidence directory. No compiler, spec or language rule changed;
all review work remains uncommitted.

## Tour follow-up: give regex operations room

DG asked to split the cramped regex slide, show one regex attest, explain
why backslashes need no shell escaping, and teach the builtin operations
before their convenience spellings. Six focused slides now cover matching,
captures, a regex attest, `sub`/`gsub`, substitution sugar and chained edits.
The capture slide also introduces `matchall`, used by the later exercise.

The examples leave simple regex words unquoted. Parentheses and counted
repetition in ordinary builtin arguments still need quotes: the initial
unquoted draft correctly received parser diagnostics, so the final capture
pattern is single-quoted. Quoting groups the argument; it does not disable
regex syntax. The attest deliberately checks for digits somewhere in its
input, then ordinary code extracts the first sequence. It does not claim
that the whole input is numeric or resolve the existing anchor bug.

The replacement examples distinguish new text from mutation. `foo =~`
takes a binding name, not a `$foo` read. Its three-edit chain demonstrates
left-to-right dependence, per-edit `g`, and continuation after a comma;
the expansion chain produces text without writing the subject.

All 55 snippets pass, including exact output assertions for the six new
slides. Five direct controls of the extracted attest source cover a number,
embedded digits, multiple sequences, a missing argument and no digits.
Presenterm's overflow check accepts the six slides at 100 columns by 30
rows and reports a deliberately oversized control. That control is judged
by the rendered overflow error, not exit status: quitting its error screen
also exits zero.

Evidence is in `tour-regex-check-final.log`, `tour-regex-results.json`,
`tour-regex-layout-final-100x30.log` and
`tour-regex-layout-negative-control.log` in the same private directory.
No compiler or spec change, new language rule, commit or push; the complete
review set remains uncommitted.
