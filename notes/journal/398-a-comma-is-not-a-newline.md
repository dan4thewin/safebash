# 398 — a comma is not a newline

The coverage queue's next item was `atok_to_text` in `parse/arith.rs`: 41 of
its 52 lines cold, only five arms ever executed. It is the function that turns
an arith token back into the spelling the user typed, so SB4020 says
``found `&` `` instead of `found Amp`. A pure `fn(&ATok) -> String` with a
48-arm match — on its face the most boring thing in the compiler.

It was hiding a wrong-answer bug.

## Reachability, by execution

The rule from the last few sessions is that a cold line is a hypothesis, not a
finding, and the way to test it is to run something. So rather than reason
about which arms *ought* to be reachable, I put every token in turn where an
operand is owed — `$(( 1 + ** ))`, `$(( 1 + <<= ))`, `$(( 1 + ) ))` — and read
the message back. Thirty-one arms answered. `expect()` turned out to be called
with only two tokens ever, `Colon` and `RParen`, and `(1 + 2` produced
``expected `)` ``, which accounted for one more.

Then the probe list reached the comma, and the compiler said nothing at all:

    $ sash -c 'echo "$(( 1 + , 2 ))"'
    sash: -c -> /tmp/p.bin (static binary)
    $ ./p.bin
    3

Bash rejects that. So do C, Go, and every language with a comma operator.
sash silently dropped the comma and evaluated `1 + 2`.

Four more shapes fell out immediately — `$(( , 1 ))` → 1, `$(( 1 ,, 2 ))` → 2,
`$(( ,, ))` → 0, `$(( 1, ))` → 1 — all rejected by bash, all accepted here,
all producing a number rather than a diagnostic. This is exactly the failure
mode the comment above the operand parser says was closed:

> Recovering silently to 0 turned a malformed expression into a *wrong answer*
> rather than an error.

It was closed for `$(( 1 + ))`. It was open for `$(( 1 + , 2 ))`.

## Why it was open

One function:

```rust
fn skip_separators(&mut self) {
    while matches!(self.peek(), Some(ATok::Sep) | Some(ATok::Comma)) {
        self.advance();
    }
}
```

Called in two places — the operand position (where DD#94's continuation rule
lets a pending operator swallow a newline) and the sequence fold. Because it
consumed a *run*, `,,` collapsed to one separator. Because it lumped `Comma`
in with `Sep`, a comma in operand position was swallowed like a line break.
And a third site, the "a run of nothing but separators is a legitimately empty
expression" escape that exists for the C-style `for (( ;; ))` header, also
admitted commas — which is how `$(( ,, ))` became `0`.

The awkward part: under DD#94 as written this was arguably *deliberate*. The DD
says `,`, `;` and newline "are interchangeable", and says the continuation rule
"falls out of one rule in the parser: the operand position skips separators".
Read literally, `$(( 1 + , 2 ))` → 3 is the documented consequence. So this
went to DG rather than getting fixed on the spot.

## DG's distinction

> I see `,` as an expression separator and `;` and newline as a statement
> terminator. However, there doesn't seem to be much difference between a
> statement and an expression, if any. The bounding `((` and `))` means that
> all intervening statements/expressions must be arithmetic.

The last sentence is true, and the distinction survives it anyway — because
what separates the two categories is not *what they join* but **how they treat
emptiness**:

- A **terminator** says "that one is done". Repeating it terminates nothing
  twice, so a run of them is harmless. Blank lines, `;;`, the empty statement.
- A **separator** asserts a thing on *each* side. An empty side is a
  contradiction. `f(a,,b)` is legal in no language.

And a second asymmetry follows from it: a pending operator may swallow a
*terminator*, because the statement is clearly not over — that is Go and JS
automatic semicolon insertion, and it is what makes `(( x = 1 +⏎2 ))` work.
Swallowing a *separator* is incoherent, because `1 +` is missing an operand by
definition. That is precisely the step that produced the `3`.

So DD#94 overreached in one specific way. Generalising the comma-fold to `;`
and newline was the whole point of the DD and was right. Then, for symmetry,
it let `,` inherit the *terminator* privileges — run-collapsing, and being
swallowed in operand position — that it never had.

## The fix

`skip_separators` becomes `skip_terminators` and consumes only `Sep`. The
sequence fold consumes at most one comma per element boundary and requires an
expression on its far side. The empty-expression escape admits terminators
only.

| input | before | now | bash |
|---|---|---|---|
| `$(( 1 + , 2 ))` | **3** | SB4020 | error |
| `$(( , 1 ))` | **1** | SB4020 | error |
| `$(( 1 ,, 2 ))` | **2** | SB4020 | error |
| `$(( ,, ))` | **0** | SB4020 | error |
| `$(( 1, ))` | **1** | SB4020 | error |

Every readability feature DD#94 was taken for is untouched, and the existing
`94.1`–`94.8` specs prove it: newline sequencing, mixed `,`/`;`/newline blocks,
the continuation rule, leading and trailing separators. Blank lines still work,
including mid-continuation. `(( a = 1 ⏎ , b = 2 ))` is still accepted — the
newline is trivia and both sides of the comma are real expressions, so there is
nothing empty about it. I had predicted to DG that this shape would have to
become an error; it doesn't, which makes the change cost-free where I expected
a small cost.

Specs `94.31`–`94.40`. The revert proof predicted five failures and got six:
`$(( 1, ))` also passed the old parser, which I had assumed already rejected it
— on the old code the trailing comma hit the "trailing separator ends the
block" path and evaluated to 1. Six new-behaviour specs fail without the fix;
the four shields pass with or without it. Sweep 6181 → **6201**, exactly as
predicted, 0 failing, 0 flips.

## And the 41 cold lines

Answered, and the answer is that most of them cannot be reached.

`atok_to_text` has exactly two callers. `expect()` is only ever called with
`Colon` and `RParen`. The operand-position fallback only sees tokens the
operand parser did not already consume — and `Num`, `Var`, `DollarVar`,
`Subscript`, `BareSubscript`, `ParamExpansion` *are* operands, `+ - ~ !` are
unary prefixes, `++`/`--` have their own error, and `LParen` opens a group.
Sixteen arms exist for variants that can never arrive.

They are not dead code in the sense that matters. The doc comment is explicit
that the match is total so a new `ATok` is a compile error rather than the next
leaked variant name, and that guarantee is worth more than the coverage number.
But totality only proves an arm *exists* — it says nothing about whether the
arm says the right thing. Nothing checked that `LtLtEq` spelled itself `<<=`.

So the arms get a unit test instead of specs: one table, every variant, every
spelling, plus a count assertion so a variant added with an arm but no case
trips a test rather than reaching a user. `mk test` 51 → 52. Coverage of the
function goes to 100% through the union run, and — unusually — the test earns
its place independently of that, because it pins the one property totality
cannot.

Fixing the comma converted one more arm from cold-and-unreachable to
cold-and-now-covered, which is a fair summary of the whole exercise: the cold
line was not the bug, but looking hard at why it was cold is what found one.
