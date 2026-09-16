# 337 — the compiler crashed on `${1[0]}`

Branch-coverage mining, three functions in, walked into an ICE. Not a wrong
answer — a panic:

```
$ sash -c 'echo ${1[0]}'
thread 'main' panicked at sash/src/codegen/types.rs:190:28:
ICE: undefined array: 1
```

`${[0]}`, `${0[0]}`, `${1[0]}`, `${9[2]}`, `${12[0]}` — every all-digit
collection name and the empty name, in both the plain and the arithmetic
context. Five one-line programs that crash the compiler.

## Why nothing caught it

The boundary is exact, and that is what named the cause:

| spelling | result |
|---|---|
| `${_[0]}`, `${a1[0]}` | SB5031 — assigned nowhere |
| `${@[0]}` | SB5045 |
| `${#[0]}` | SB4038 |
| `${[]}` | SB4041 |
| `${1[0]}`, `${[0]}` | **ICE** |

Everything with a *name* is diagnosed. Everything with a *sigil* is diagnosed.
Only a name that is not a name falls through.

`record_param` (validate.rs:6954) is the typo check, and it skips deliberately:

```rust
if opt_out || !is_plain_identifier(name) || handled_elsewhere(name) || known.contains(name) {
    return;
}
```

That skip is right. `$1` and `$@` are not typos; they are handled by SB5024 and
SB5045. But the skip is written for a *scalar read*, and it also silently
excuses a **subscript's collection name** — and nothing else ever checks that
one. So the AST reached `find_array_slot`, which looks up `1_0`, finds nothing,
and panics.

Meanwhile the scanner that produced the name is looser than the language:

```rust
fn scan_param_name(content: &str) -> (&str, usize) {
    while name_end < bytes.len() && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
```

A run of `[A-Za-z0-9_]` — which admits a leading digit, and admits the empty
run. `is_plain_identifier` (ir.rs:408) is the real predicate and requires the
first byte to be alphabetic or `_`. Two definitions of "name", and the looser
one was doing the parsing.

## The fix — DG's option C

I put three options to DG: a new validate rule with its own code, relaxing
SB5031's skip, or rejecting at parse reusing the existing braced-body family.
DG chose **C**.

It turned out to need only one diagnostic site. The arithmetic path has its own
scanner, `try_braced_subscript`, whose name test was the same loose
alphanumeric run. Tightening it to `is_plain_identifier` does not need a
diagnostic there — returning `None` routes the body to the real parser (DD#77),
which is the same `try_subscript` that handles the plain spelling. So both
contexts converge on one reject:

```rust
if !is_plain_identifier(name) {
    return Err(bad_subscript_name_diag(content, name, span));
}
```

SB4047 is the right code to reuse because its stated principle is exactly the
one the ICE violated — *"the compiler must never emit code for input it doesn't
fully understand."* An ICE is the loudest possible form of not understanding.

The message names the rule the way SB5011 already words it:

```
`${1[0]}` has no collection to subscript — `1` is not a variable name.
A name is letters, digits and `_`, and cannot start with a digit
```

with a separate clause for the empty case, where saying "`` is not a variable
name" would name nothing.

## Specs

`var_modifiers.sh` **vm.un7–un12**, joining the existing `vm.un1–un6` SB4047
family. Five rejects — all three digit spellings plus the two arithmetic
paths — each asserting `$err != *ICE*` as well as the code, so a regression
that panics again fails on the panic rather than passing on a coincidence.

`vm.un12` is the guard in the other direction: `array a1=(7 8); echo ${a1[1]}`
must keep working. The reject is a *leading* digit, not any digit, and a
predicate change like this is exactly the kind that over-rejects quietly.

Sweep **5165 passing, 10 skipped, 0 failing** — 5153 + 6 × 2 modes, predicted
exactly. Units 83 green.

## The loop is working

DG's reason for the 100% branch-coverage target was that it is "our best chance
of eliminating all the dead code and finding bugs while they are cheap to fix."
Three sessions in, the ledger is a ghost operator, three dead predicates, two
`unset` bugs, and now an ICE — none of which the 5000-test suite was failing
on, and none of which anyone went looking for. The cold branches are where the
compiler has never been asked a question.

Worth noting what actually found this one: not the cold branch itself. I was
probing `try_braced_subscript`'s three cold branches with deliberately
malformed spellings, and two of the probes crashed. The value of the target was
that it made me type `${[0]}`.
