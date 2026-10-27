title: DD#31 — Arithmetic mutations and comparisons are disjoint
status: implemented

Three arithmetic contexts, each with a single purpose:

| Context | Purpose | Accepts | Rejects |
|---|---|---|---|
| `(( ))` statement | Mutation | `=`, `+=`, `-=`, `x++`, `x--` | Comparisons, prefix `++x`/`--x` |
| `if (( ))` condition | Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` | Mutations |
| `$(( ))` expression | Pure evaluation | Arithmetic on clean operands | Mutations |

`$(( ))` is a read context — it returns a value. Mutation inside a
read context (`$((x=10))`) is invalid sash (compile error).

## Increment/decrement (Go rule)

Only postfix `x++` and `x--`. No prefix forms. Statement-only —
never produces a value. These are syntactic sugar for `x = x + 1`
and `x = x - 1`.

Valid:
- `(( count++ ))` — standalone mutation statement
- `(( x++, x > 0 ))` — non-final position in comma expression

Invalid:
- `(( x = y++ ))` — `++` inside expression (not a statement)
- `(( ++x ))` — prefix form rejected
- `if (( x++ ))` — mutation in condition position

## Comma operator

The comma in `(( ))` is syntactic sugar that splits one `(( ))` into
multiple statements. Each non-final expression must be a mutation.
In condition position (`if`/`while`), the final expression must be a
comparison:

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

Enforcement is shared, so the diagnostics are too. A comma holding a mutation
anywhere classifies as a mutation and is split into statements before
validation; whether the final part is a comparison is then decided on the
split form by the ordinary mutation-in-condition rule (SB5007), the same one
that rejects `while (( k=1 ))`. Only the other half — every non-final part
must be a mutation — is checked on the comma itself, because nothing else
can see it.

## Bash-mode coercion

- `$((x=10))` → `((x=10))` (mutation extracted to statement form)
- `$((x=10, x<y))` → `((x=10)); $((x<y))` (compound split: mutation
  to statement, comparison stays as expression in context where it
  has meaning)

Statement `(( ))` exit status is always 0. Condition `if (( ))`
produces a boolean. `(( x > 0 ))` as bare statement → compile error.
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

The write need not come first: `(( a<3, a++ ))` gets the same answers. A `-`
in front changes nothing, and neither does a `!` before a redirect.

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

- The non-final rule is checked only on a comma that is still unsplit, in a
  condition. So `(( a<3, a>1 ))`, `(( a, a++ ))` and `if (( a, a++, a<3 ))`
  compile, and `if (( a<3, a++, a<3 ))` is given the bare-statement advice.
  "Enforcement is shared" above claims more than the code does. Filed as
  `bug-arith-comma-nonfinal-part-unchecked`.
- A lone test statement, such as `(( a ))` or `(( 0 ))`, compiles, sets `$?`
  from its value and does not abort. The suite uses `(( 0 ))` as its
  non-aborting failing status. That conflicts with the table above and with
  "exit status is always 0". Filed as the question
  `q-arith-test-statement-status`.

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
