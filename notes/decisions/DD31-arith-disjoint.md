title: DD#31 — Arithmetic mutations and comparisons are disjoint
status: implemented

Three arithmetic contexts, each with a single purpose:

| Context | Purpose | Accepts | Rejects |
|---|---|---|---|
| `(( ))` statement | Mutation, no result | `=`, `+=`, `-=`, `x++`, `x--` | Comparisons, bare tests/values, prefix `++x`/`--x` |
| `if (( ))` condition | Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` | Mutations |
| `$(( ))` expression | Pure evaluation | Arithmetic on clean operands | Mutations |

`$(( ))` is a read context — it returns a value. Mutation inside a
read context (`$((x=10))`) is invalid sash (compile error).

## Results and statement use (DG, 2026-11-08)

**Assignments and mutations produce no result. Comparisons do.** A write
does not yield its assigned value, a truth value, or an always-zero
success status. This applies to DD71's value bindings and to arithmetic
assignment, compound assignment and increment/decrement.

Reject source `-` on an arithmetic mutation: there is no result to
discard. A comparison must be consumed in a permitted context, not left
as an arithmetic statement; `-` cannot make that statement legal either.
The same statement restriction rejects bare tests and calculations such
as `(( 0 ))`, `(( n ))` and `(( n + 1 ))`. They are not a way to manufacture
a readable status without a command.

```sash
(( n += 1 ))                # mutation statement, no result
if (( n > 0 )) { echo yes }  # comparison result consumed
- (( n += 1 ))              # rejected: no result to discard
(( n > 0 ))                 # rejected: unused comparison
- (( n > 0 ))               # rejected: discard cannot legalize it
(( n = (m += 1) ))          # rejected: the inner write supplies no value
```

Parentheses alone do not turn a write into a result-producing test.
An otherwise legal parenthesized mutation follows the same statement
rule; using that write as the value of another expression is invalid.
Comma sequencing remains the distinct desugaring below, not an
assignment-expression result.

Writes establish no fresh `$?`/`$(status)` permission. Reading an already
available status in an operand still follows DD55's handling and
first-expression rules. DD88/DD101 capture conditions consume the captured
operation's result, not a result of assigning its value.

This is a result/context rule, independent of DD70's conservative
pointless-statement check. Operand effects or uncertainty do not turn an
arithmetic comparison into a legal statement. Ordinary operand checks
and fatal arithmetic faults remain unchanged. An outer prefix cannot
discard an operand's failure, and fatal faults remain fatal.

**Enforcement pending:** `q-arith-test-statement-status` retains its slug
and P3 priority but is now an implementation bug. It owns this arithmetic
contract and the migration of status-source fixtures. Ordinary assignment
prefixes remain with `reject-nonadjacent-suppression`; the comma tasks
retain their shared classification and list-placement work. Journal 593
records the ruling; no compiler change is claimed here.

## Increment/decrement (Go rule)

Only postfix `x++` and `x--`. No prefix forms. Statement-only —
never produces a value. These are syntactic sugar for `x = x + 1`
and `x = x - 1`.

Valid:
- `(( count++ ))` — standalone mutation statement
- `if (( x++, x > 0 ))` — mutation in a non-final comma part, comparison consumed

Invalid:
- `(( x = y++ ))` — `++` inside expression (not a statement)
- `(( ++x ))` — prefix form rejected
- `if (( x++ ))` — mutation in condition position

## Comma operator

The comma in `(( ))` is syntactic sugar that splits one `(( ))` into
multiple statements. In statement position every part must be a mutation.
In condition position each non-final part must be a mutation and the final
part must supply the comparison:

```bash
# desugars to: (( count = count - 1 )); if (( count > 0 ))
while (( count = count - 1, count > 0 )); do
```

Equivalent to `while` with multiple statements before `do`, using
the final as the condition.

**"Condition position" is all four of them.** `if`, `while`, `until`, and
the test slot of a C-style `for` header are one rule, not four. The header's
other two slots are not a substitute: `for (( ; k<3; k++ ))` tests before it
increments and prints `0 1 2`, while `for (( ; k++, k<3; ))` increments
first and prints `1 2`. The equivalent without the comma writes the mutation
twice, `for (( k++; k<3; k++ ))`.

The non-final rule must hold before and after splitting, and parentheses
must not disguise a mutation as a test. The final part obeys its enclosing
context; a preceding write cannot make a discarded final comparison or
calculation legal. The existing enforcement misses these distinctions in
some forms, as recorded below and in `bug-arith-comma-nonfinal-part-unchecked`.
The 2026-11-08 ruling settles the result contract, not the separate question
of which logical-list positions accept an arithmetic comma.

## Bash-mode coercion

- `$((x=10))` → `((x=10))` (mutation extracted to statement form)
- `$((x=10, x<y))` → `((x=10)); $((x<y))` (compound split: mutation
  to statement, comparison stays as expression in context where it
  has meaning)

Extracting a mutation does not give it a result. Condition `if (( ))`
produces a boolean; a mutation statement produces none, not status zero.
`(( x > 0 ))` as bare statement → compile error.
`if (( x++ ))` → compile error.

## Addendum (2026-10-27): a comma with a write is an assignment, not a compound

*Decide-and-report; DG may veto.*

The comma splits one `(( ))` into statements, and the parser keeps those
statements together in one group so that a condition can take the last part.
Wherever a postfix was checked, that group was classed with `{ … }` and `if`.
So `(( a++, a<3 )) | cat` was told to wrap a compound in a function, and a
redirect on one was a compound's SB4021. In a return producer
(`return (( n = 1, n > 0 )) | cat`) the group crashed the compiler, until
DD#70's 2026-10-26 addendum gave it those same compound answers. The user wrote
one arithmetic command.

**Mutation wins.** The group exists only because a part writes. Wherever an
assignment may not stand, it gets the assignment's answer, as `(( a = 1 ))` would:

| Where | Answer |
|---|---|
| a pipeline stage: first, middle or last | SB4022 "a variable assignment isn't allowed as a pipeline stage" |
| before a redirect: as a statement, before `&&`, under `!` | SB4046 "a redirect cannot attach to a variable assignment" |
| under `&` | SB4024 "a variable assignment isn't allowed in a background job" |

The write need not come first: `(( a<3, a++ ))` got the same answers in
these probes. A `-` did not change those placement diagnostics; DD31's
2026-11-08 rule now independently rejects that prefix. A `!` before a
redirect does not license a redirect on a write.

**Held: `&&` and `||`.** Before or after `&&`/`||`, the group keeps the
compound's SB4023 for now. Whether that operand is a condition position is
open, and a ruling decides what the group means there:

- A plain write before `&&`/`||` is SB5007, mutation in a condition:
  `(( a++ )) && echo y` and `(( a++ )) || echo y`. That treats the operand as
  a condition.
- Under `!` and in a return producer the group already compiles, and its last
  part decides the list: `! (( a++, a>3 )) && echo y` and
  `return (( a++, a<3 )) && echo y` both print `y` and leave `a=1`.
- As a statement or a list operand it is SB4023:
  `(( a++, a<3 )) && echo y` and `/bin/true && (( a++, a<3 )) && echo y`.

If the operand is a condition position, this section's "all four of them"
gains a fifth, and the SB4023 cases compile as the `!` case does. Until DG
rules, the parser keeps the old answer at that one position rather than
inventing a new one. The question is filed as
`bug-arith-comma-classified-compound`.

**Outside this addendum.** The probes for this addendum found two older gaps
in DD#31's own rules. Both are in validator functions this change does not
touch, so both are filed rather than fixed:

- The non-final rule was checked only on a comma that was still unsplit, in a
  condition. So `(( a<3, a>1 ))`, `(( a, a++ ))` and `if (( a, a++, a<3 ))`
  compiled, and `if (( a<3, a++, a<3 ))` was given the bare-statement advice.
  The former "enforcement is shared" description claimed too much. Filed as
  `bug-arith-comma-nonfinal-part-unchecked`.
- A lone test statement, such as `(( a ))` or `(( 0 ))`, compiled, set `$?`
  from its value and did not abort. The suite used it as a non-aborting
  status source. DG rejected that statement form on 2026-11-08 and settled
  mutations as no-result writes, replacing the former "exit status is
  always 0" claim. `q-arith-test-statement-status` now owns enforcement;
  these are historical measurements, not a new fixture census.

**Mechanics.**
- The parser's compound test no longer counts the group.
- The `&&`/`||` hold is a check of its own.
- The statement-form and redirect labels call the group "a variable
  assignment".
- The background check puts it with assignments and declarations.
- `wrap_pipeline_part` takes a statement as a stage or a list operand. It now
  panics (an ICE) if a compound reaches it: each compound's own postfix check
  rejects it first, and a compound let through would be built into a pipeline
  silently.

Specs: errors.sh acm.1-acm.12. csfx.9 and csfx.11 keep their ids and now pin
the assignment answers. Journal 537.
