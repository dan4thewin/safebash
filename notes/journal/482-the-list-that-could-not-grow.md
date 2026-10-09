# 482 — the list that could not grow

The standing task was titled "only `len` works" and marked **done**. It was
not. What closed in August was category A (int intrinsics compute) and the
part of category B someone could enumerate at the time. The enumeration is
the bug.

## The measurement

`$(( (echo) ))` compiled and printed `0`. So did eighteen others:

```
echo write exit exec cd wait close attest read readline mapfile
readarray argv break continue return signal unset matchall
```

Nineteen, not the twelve I had carried forward in my notes. I had been
quoting a list too, and mine had rotted the same way — it predated `write`,
`readline`, `mapfile`, `attest` and `matchall`. Measuring the whole cohort
rather than re-checking the twelve is what corrected it: I ran every name in
`BuiltinCmd::from_name` ∪ `resolve::INTRINSICS` through
`int x=$(( (NAME) ))` and read the column.

## The cause: a third list, pointing the wrong way

`type_check.rs::is_sexpr_string_producer` held

```rust
const STRING_INTRINSICS: &[&str] = &[
    "toupper", "tolower", "join", "keys", "basename", "dirname",
    "getcwd", "getenv", "arg", "sub", "gsub", "match",
];
```

and said: a sexpr callee is wrong if it is on this list, or is a user string
function. Everything else — including every compiler name that denotes no
value at all — fell through as a legal int producer, into
`resolve_nullary_sexpr`, out through its variable branch, and into a read of
an unset slot.

That is **allow-by-default over an open set**. The set is open because
`BuiltinCmd` grows: five of the nineteen are variants added after the list
was written, and every one of them joined the "valid int producer" class on
the day it was declared, silently, with no edit to this file.

This is the same shape as `is_scalar_value_intrinsic`, which `resolve.rs`
already carries a paragraph about — it was inverted years ago for exactly
this reason, and the comment explaining why sat one file away from the list
repeating the mistake.

## Three lists for one question

Counting, the name set was described in three places that could disagree:

| where | shape | answers |
|---|---|---|
| `resolve::INTRINSICS` + `BuiltinCmd::from_name` | union | "is this name the compiler's" |
| `type_check::is_int_returning_intrinsic` | allow-list of 10 | "is `$(name)` int-typed" |
| `type_check::is_sexpr_string_producer` | **deny**-list of 12 | "may `(name …)` appear in `$(( ))`" |

The second and third are the same question asked twice. Asking it twice is
what let the second answer rot; the first answer stayed right because
`is_intrinsic` is a union and a union of two growing sets grows.

`INTRINSICS` had its own drift, in a direction nobody had to care about
until now. Its doc says it lists names "that are NOT PATH-resolved externals
**even though `from_name` returns `Other` for them**" — and five of its
sixteen entries (`argv`, `len`, `strlen`, `keys`, `join`) are names
`from_name` recognizes perfectly well. Harmless while every reader only
wanted the union. Not harmless the moment a name has to carry a *denotation*,
because then two tables hold an answer and nothing makes them agree.

## The fix: key it on the enum

One table, `resolve::ArithDenotation`, with three answers — `Int`, `Str`,
`Statement` — and a classifier that is **total over `BuiltinCmd`**. A new
builtin does not compile until someone says what it denotes. That is the
property a deny-list cannot have.

`Other` is a real answer in that match, not a hole: it means "not the
compiler's name", which is how a plain variable (`$(( (x) ))` is grouping,
not a call) and a user function reach their own resolution. So the function
returns `Option` and has no unreachable arm except the nine internal opcodes,
which are a pure totality arm.

Both of the other two lists are gone. `is_int_returning_intrinsic` is now
`arith_denotation(name) == Some(Int)`, and `is_sexpr_string_producer` is
replaced by a classifier that returns the error, because the three answers
need two different messages:

- `Str` keeps the old text — "it yields a string… capture it first with
  `$()`" — which is good advice.
- `Statement` needs its own, because that advice is wrong for `break` and
  `cd`: "`echo` is a command, not a value — it denotes nothing in
  arithmetic."

The five duplicate `INTRINSICS` entries are deleted, and the two call sites
that spelled the union by hand (`is_resolvable_external`,
`is_external_looking`) now call `is_intrinsic`, which is what they meant.

## What I did not change, and why

`validate::ReadSink::is_sexpr_callee` still calls `is_intrinsic`. My notes
said that line was the conflation site. It is not. It decides "is this name
a *variable read*", and the answer for `echo` is no — DD#125 guarantees
`echo` can never be a variable, so routing it to SB5031 ("assign it first")
would hand the user advice that SB5092 forbids them from taking. Letting it
through as a callee and rejecting it one pass later, on type, is the correct
layering. The doc on `is_intrinsic` now says so, so the next reader does not
re-conflate them.

## The backstop that does earn its line

`resolve_nullary_sexpr`'s fallthrough now ICEs for any name with a
denotation. The doc above it already recorded the historical failure:
`getcpid` was once absent from the explicit arm and silently read an unset
slot, invisibly, because `validate` exempts intrinsic names from SB5031.

It qualifies under the orthogonality rule. The gates upstream — `type_check`
rejecting `Str`/`Statement`, SB4045 rejecting a nullary `(len)` — look at the
*denotation table*. This arm looks at the transform's own *name list*. Only
this one can notice an `Int` entry added to the table and never given a
lowering, which is precisely the `getcpid` regression. A unit test pins both
shapes it could arrive in.

## Gates

Sweep **7883**, predicted before running. Zero failing, zero flips, 36 new
ids (18 × 2 modes), gate self-test 248. Units 86/27/20.

The zero-flip result on the *first* sweep — before any new spec — is the
finding that sizes the gap: no spec in the suite touched any of the nineteen.
The behaviour had no coverage at all, in either direction, which is how a
task could be marked done with two thirds of its own category still open.

## Method note

The task file's gate list is worth reading as a specimen. It names thirteen
string intrinsics by hand and says "spec tests pin every func's sexpr
behaviour". Both halves are enumerations, and both were satisfiable by a fix
that enumerated. A gate written as a list can only ever check the list; a
gate written as "the classification is total" checks the ones nobody has
thought of yet.
