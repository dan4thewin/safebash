title: "Require `in` and source items in word-list `for`; reject `;` before `in`"
status: open
priority: P2
kind: work
depends: []
gates:
  - "word-list loops without in, with a semicolon before in, or with an explicitly empty source list receive located refusals in native and C, independent of taint switches"
  - "brace and keyword bodies obey the same rule; otherwise legal body placement, marks or redirects do not excuse an invalid header"
  - "newline before in, the keyword separator after a populated list, C-style for grammar, a single empty-string item and runtime-empty data retain their existing semantics"
  - "sash-convert rewrites Bash's omitted-list argument loops with in @args in functions or in $(argv 1) at top level, preserving both default and canonicalized body spellings"
  - "a genuinely empty Bash in list is not converted into argument iteration or silently erased to evade the error"
  - "current fixtures are measured before migration; changed contracts use fresh global ids and focused before/reversion controls pin each refusal and the converter repair before the predicted gate passes"

## Decision settled, 2026-11-09

DG ruled:

> yes to require in; reject the stray semicolon; an explicitly empty
> list does no work - pointless code should be an error, as it must
> be an unintended mistake

DD68 is authoritative, DD70 records the pointless-code rationale,
and DD75 records the argument/converter boundary. Journal 598 records
the discussion. This task keeps its slug and P2 priority as open
implementation work; both policy questions are closed.

## Accepted rule

A word-list `for` requires `in` and at least one source item. Reject
an omitted `in`, a semicolon between the binder and `in`, and a written
`in` with no items. The last case is an error too, not the exception
the discussion initially proposed. It is not useful merely because
its unreachable body contains commands or writes.

Apply the rule to both brace and keyword bodies, including across
newlines and in nested executable bodies. Marks and redirects cannot
legalize the header. Preserve a newline before `in`, the ordinary
separator after a populated keyword-form list, and C-style
`for ((init; cond; incr))`; C-style loops do not use this list grammar.

Source-list emptiness is not runtime cardinality. A written list can
yield no items at runtime without a new error, and `""` supplies one
empty-string item. This ruling adds no requirement to prove or check
that input data is nonempty. Normal typing, taint, binding, return,
body-flow and control-transfer rules remain.

## Converter obligation

Bash's omitted-list form iterates arguments; sash must not silently
retain it as an empty loop. Convert all three recorded spellings to an
explicit `in @args` in a function or `in $(argv 1)` at top level,
using the existing DD75/DD60 forms and keeping their normal checks.
Do this in both default keyword-preserving and canonicalized output.

Do not conflate an omitted list with Bash's explicitly empty `in`
list. The latter must not acquire arguments or disappear merely to
produce accepted sash; surface the invalid construct through the
existing conversion/diagnostic workflow. No new conversion mode,
diagnostic number or compiler architecture is selected here.

## Historical measurements (2026-10-27; not rerun)

Found 2026-10-27, while porting F1+F4 (journal 537). `parse_for`
(`sash/src/parse/parser.rs`) is untouched by the port, and every result below
is the same at `8e28c61` and with F1+F4, in both backends.

### Q1 evidence: a `for` with no `in` list

`parse_for` treats the `in` list as optional, and a missing list is an empty
one, so the loop never runs. DD#68's Forms listed only `for NAME in LIST { … }`,
and no DD then specified the shorter form. In bash it iterates `"$@"`.
DD#75 §5 removed `$@` and spells that iteration `for x in @args` in a
function and `for x in $(argv 1)` at top level (DD#60's argv[1:] splat).

| Id | Program, run with arguments `p q` | sash | bash |
|---|---|---|---|
| g7 | `for x; do echo "$x"; done` | prints nothing, rc 0 | `p`, `q` |
| g9 | `for x`, newline, `do echo "$x"; done` | prints nothing, rc 0 | `p`, `q` |
| g11 | `for y do echo "[$y]"; done` | prints nothing, rc 0 | `[p]`, `[q]` |
| g10 | `for x { echo "$x" }` | prints nothing, rc 0 | not bash |
| f1 | `fn f { for x { echo "[$x]" } }`, then `f a b` | SB5043 4:1 "function 'f' takes exactly 0 arguments but was called with 2" | not bash |
| g12 | `for x in; do echo "[$x]"; done`, then `echo end` | `end` | `end` |

g12, an explicit empty list, agreed with Bash but now requires an error
under DG's ruling; it is not a preservation case. In a function the old
arity check caught f1, but its message blamed the call, not the loop.

sash-convert passes `for x; do` through unchanged and adds no MANUAL note
(f2). A converted bash loop over the arguments therefore does nothing at top
level, and in a function it draws f1's SB5043. `for y do` gets "MANUAL: could
not parse `for y`". The explicit `for y in "$@"` converts correctly: it
becomes `@args` in a function and `$(argv 1)` at top level (f3, f4 print
`[p]`, `[q]`).

### Q2 evidence: a `;` before `in`

`parse_for` skips any `;` or newline between the variable and `in`
(`skip_terminators`).

| Id | Program | sash | bash |
|---|---|---|---|
| g5 | `for x; in a b; do echo "$x"; done` | `a`, `b` | "syntax error near unexpected token `in'" |
| g6 | `for x`, newline, `in a b; do echo "$x"; done` | `a`, `b` | `a`, `b` |

Bash's grammar takes a newline before `in` but not a `;`, because there
`for x;` is the no-list form of Q1.

Filed under DG's rule for the port that questions and uncovered cases become
backlog tasks.

## Implementation evidence and ownership

Retire the earlier recommendations to keep the semicolon or preserve
an explicitly empty list. Neither is the accepted contract. Keep the
tables as historical evidence, not current gate predictions.

Pin missing `in`, the stray semicolon and the explicitly empty list
separately, naming the actual reason and location rather than merely
asserting compilation failed. Use otherwise legal binders and ordinary
statement positions so an unrelated name, result-use, group or
suppression refusal cannot satisfy the assertion.

Preserve g6's newline continuation, populated brace/keyword loops,
an empty-string item, runtime-empty inputs and C-style semicolons.
Exercise the converted top-level and function argument loops with
multiple and zero arguments, including the historical f1 arity edge.
The converter must repair the iteration source, not hide the resulting
call error or assume all loops are at top level.

`reject-pointless-statements` retains the general ignored-result
effect/use check. This task owns the explicit empty-header refusal;
neither implementation needs to wait for the other. An early header
error is not proof of that effect analysis, and a later unrelated
diagnostic is not proof of this header rule.

Take a current fixture census before changing specs. Use fresh ids
above the database's historical high-water marks for changed
contracts, prove the targeted refusals and converter behavior with
before-binary and appropriate independent reversion controls, and
predict the ordinary gate from the measured impact. No compiler,
converter or spec change was made during this recording.

Probes: `files/port-f1f4-probes.tar.gz`, directory `qq/` (g5-g7, g9-g12,
f1-f4).
