title: DD#105 — a string intrinsic rejects an int-typed operand
status: accepted
depends: [DD16, DD65, DD102]

Settled with DG 2026-09-13.

A string intrinsic applied to an int-typed operand is a **type error**
(SB4030). It is not silently rendered as text.

```sh
int n=42
echo $(toupper $n)        # SB4030 — was "42"
echo $(strlen $n)         # SB4030 — was 2
echo $(strlen "$n")       # 2      — explicit conversion, still fine
echo $(strlen 42)         # 2      — a literal is a string (DD#16)
```

## Why

DD#16 makes int an explicit annotation precisely so that int and string
do not silently interconvert. Every *other* mistyped operand already
said so: `join`'s collection operand is SB4030, `len`'s scalar operand
is SB4030, a substring of an int (`${n:1:2}`) is SB4030, an int
redirect target is SB4030.

The string intrinsics were the hole. `$(toupper $n)` on `int n=42`
yielded `"42"` — the compiler quietly chose a decimal rendering the
source never asked for. `$(strlen $n)` was worse than useless: it
answered *"how many digits does 42 have"* while looking exactly like a
question about a string's length. An implicit int→string coercion in a
language whose type annotation exists to prevent one.

DG: *"strlen should behave like toupper … string using functions should
also give an error for int variables."*

## Scope — the operand's TYPE, not its spelling

DG chose the **broad** reading. The rule keys on `expr_type`, so it
catches every way an int can arrive:

| operand | verdict |
|---|---|
| `$n` where `int n` | SB4030 |
| `${a[0]}` where `int array a` | SB4030 (DD#76 element type) |
| `$(( … ))` | SB4030 |
| `$(getpid)` and the int-returning intrinsics | SB4030 |
| `"$n"`, `"$(( … ))"` | **allowed** — a double-quoted word is a string |
| `42` (bare literal) | **allowed** — bash numerics are strings (DD#16) |

Keying on the type rather than the spelling is what makes the rule
coherent: `$(strlen $(( n * 2 )))` is the same mistake as
`$(strlen $n)`, and a rule that caught only the named-variable case
would be an accident of spelling.

**Double-quoting remains the conversion**, which is not a loophole but
the point — it is the same escape the pre-existing substring diagnostic
already recommends (`s="${n}"` then `${s:1:2}`). The coercion has to
become visible in the source; it does not have to become impossible.

## The checked slots

Every string slot is a *leading* argument, so a count suffices:

| intrinsic | string slots |
|---|---|
| `toupper` `tolower` `strlen` `basename` `dirname` `getenv` | 1 |
| `match` `matchall` | 2 (subject, pattern) |
| `sub` `gsub` | 3 (subject, pattern, replacement) |

Not just the subject: `$(gsub $s "b" $n)` puts an int in the
*replacement* and is equally a coercion. The trailing array target of
the 3-arg `match`/`matchall` form is excluded by the count.

`join` is absent deliberately — its first operand is a collection
(SB4030 already) and its separator must be a literal (DD#106), so
neither slot can hold an int.

## Consequences

- `check_string_intrinsic_operands` in `type_check.rs`, registered in
  **both** `check_command` and `check_subscript_in_stmt`. The second is
  the one that matters: a command inside `$( )` is reached via
  `check_subscript_types`, not `check_command`, which is why `join`/
  `len`/`keys` are registered twice too. Registering only the first
  silently does nothing.
- Type errors in `type_check.rs` render as SB4030 automatically, so the
  rule needed no new diagnostic code and lands in the same family as
  the mistyped-operand errors it completes.
- Specs `sint.1`-`sint.16` in `spec/errors.sh`. Zero corpus impact: no
  program in `spec/`, `sash-convert/` or `contrib/` applied a string
  intrinsic to an int operand.
