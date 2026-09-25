---
title: DD#118 — `==` is the only equality, `-n` is never implied, and a vacuous test is an error
status: accepted
---

Settled with DG (2026-09-25). DG's framing: *"erring on the side of clarity,
we want the vacuity rule, the `-n` test as explicit and not implied, and `==`
the only spelling of equality."*

Three changes to the `[[ … ]]` surface, each closing a way for a test to read
as one thing and mean another. They are filed together because they were
settled together and because the first two are what make the third tractable.

## How it started

DG asked whether a single `=` has two meanings. It does — **assignment** in
statement position, **string equality** in a `[[ ]]` operator slot — but the
positions are disjoint, so there is no genuine ambiguity for the parser. The
ambiguity is for the *reader*, and probing it turned up something worse.

`=` binds as assignment only when it is **symmetric**: glued both sides
(`x=y`) or spaced both sides (`x = y`). That rule is deliberate — commit
`e48f3f0`, for DD#86's frictionless write (`msg = hi`) — and its recorded
caveat was that ``[[ $x =b ]]`` still evaluates as `= b`. The sharper
consequence had not been recorded:

```sh
[[ $x = y ]]   # F — three tokens, a real string comparison
[[ $x=y ]]     # T — ALWAYS. one word, `-n "$x=y"`, and that word can
               #     never be empty because it literally contains "=y"
```

Both are what bash does, so neither was a divergence, and that is exactly why
neither was caught. The second is a test that cannot fail, written in a shape
that looks like a comparison.

## Part A — `==` is the only spelling of equality

A single `=` in a test operator slot is **SB4067**:

```
`=` is not a comparison operator; write `==` for string equality
(a single `=` always assigns)
```

A bare `!` after an operand gets the same code, directing to `!=`.

This costs nothing in expressiveness — `==` was already accepted and already
the dominant spelling — and it buys a one-to-one reading: **`=` assigns,
`==` compares, everywhere, with no whitespace rule to recall.**

## Part B — `-n` is explicit, never implied

`[[ word ]]` as shorthand for `[[ -n word ]]` is retired. A bare word in a
test is **SB4068**:

```
a bare word is not a test; write `-n WORD` to test that it is non-empty,
or `-z WORD` that it is empty
```

The implicit form is what turns Part A's `[[ $x=y ]]` from a syntax error into
a silent constant. Retiring it means the glued spelling is now *rejected*
rather than quietly true, which is the whole point: the two shapes stop
disagreeing and start being one legible thing.

Measured before adopting: the spec suite contained **zero** genuine uses of
the implicit form in sash programs. It survived only in the bash harness,
which is not sash.

### The interrupted-word case

A bare word reached with the scanner *not* at a test terminator is not a
"bare word test" at all — some other token ended the word early, and that
token is the real fault. `[[ a&b == x ]]` reports **SB4014 `expected ']]'`**,
not SB4068, because `&` is what broke it. SB4068 fires only when the test is
otherwise complete.

## Part C — a vacuous test is an error

A test whose result is fixed by the source text is **SB5086**. This catches
the residue Parts A and B cannot:

```sh
[[ -n foo ]]        # always true
[[ -z "" ]]         # always false
[[ foo == foo ]]    # always true
[[ 1 == 2 ]]        # always false
[[ a==a { … } ]]    # dispatch on a literal subject — the arm is chosen at compile time
```

None of these are comparisons; they are constants wearing a comparison's
clothes. Every one of them was in the spec suite, and in every case the thing
under test was reachable without the vacuity.

### Deliberately syntactic, never const-propagating

DG's proposed escape is the mechanical rewrite:

```sh
[[ foo == foo ]]   →   s=foo; [[ $s == foo ]]
```

That rewrite **must** remain valid, so the rule is defined on the source text
and stops at the first variable. It does not ask whether `s` is knowable; it
asks whether the operand was *written* as a constant. (There is no const-prop
of string assignments today, but the rule does not depend on that staying
true — it is syntactic by design, not by coincidence.)

Emptiness is derived only from what the word says:

| word shape | emptiness |
|---|---|
| literal / single-quoted | `Never` if non-empty, else `Always` |
| arithmetic `$(( … ))` | `Never` (always renders at least one digit) |
| double-quoted / concat | `Never` if any part is `Never`; `Always` if all parts are; else `Unknown` |
| anything else | `Unknown` |

### What the rule does not cover

- **Numeric operators** (`-eq`, `-lt`, …) are excluded, so SB5086 cannot
  shadow SB5019's more specific diagnosis of the same code.
- **File tests** (`-f`, `-d`, `-nt`, …) are excluded: `[[ -f /etc/passwd ]]`
  has a static operand but a runtime answer.
- String operators `==`, `!=`, `=~`, `<`, `>` are covered on both operands.

### Dispatch

DD#66 dispatch (`[[ subj pat { … } ]]`) is a separate IR node (`StmtKind::Case`)
and needed its own check — it is vacuous when the subject and *every* pattern
are static. The default `|| { … }` arm carries no patterns, which is vacuously
static and correctly so.

## Cost

Forty-four spec sites, all mechanical: 55 `=`→`==`, one implicit→`-n`
(`types.sh` 62.15, which was specifying the retired surface), and the rest
DG's variable rewrite. Three sites in `operand_words.sh` are worth noting —
`ow.14`-`ow.16` pin that an adjacent `&&` *terminates* a `=~` pattern, and
their own comments explained the result using the implicit form. They now
read `[[ $x =~ a&&-n $y ]]`, which pins the same structural claim without it.

## Not settled here

`[[ $x==a ]]` answers F in sash and T in bash (`extract_compact_operator`
splits the glued operator). That divergence predates this DD and is separate
from the deliberate no-space extension `[[a==a]]`, which bash cannot parse at
all and which `dispatch.sh` specs 28 times. Raised with DG; open.
