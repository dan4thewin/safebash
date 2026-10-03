---
title: "a cell takes no brace-less disposition — `$a[0]:-z` is literal concat while `${a[0]:-z}` is the disposition"
status: open
priority: P2
kind: decision
depends: []
gates:
  - "DG rules: a cell is a disposition subject brace-less (extend DD#90's scope list), or it is deliberately not (codify it, and decide what `$a[9]:?m` should do)"
  - "whichever way, `:-`, `:?` and `:~` agree with each other on a cell — today all three silently fall to literal concat"
  - "specs pin the chosen reading for an array cell and a map cell, in unquoted position"
---

## The question

DD#90 made the braces optional on a disposition expansion. Its scope list names
**a scalar `$name`, a positional `$N`, and an array splat `@name`**. A *cell* —
`$a[K]` or `$m[K]` — is not in that list, and does not get one:

| spelling | result |
|---|---|
| `echo ${a[0]:-z}` | `p` — the disposition |
| `echo $a[0]:-z` | `p:-z` — the cell, then a literal `:-z` |

Note this is **not** a parse failure or a decline-to-subscript. The subscript
parses fine (it always did, and since 2026-10-03 it parses identically in all
three spellings). What does not happen is the *disposition*: `:-z` is read as
the ordinary literal colon-concatenation DD#90 deliberately preserves for every
`:X` that is not `:-` / `:?` / `:~`.

## Why this is more than an ergonomic gap

Measured 2026-10-03 against `83d0710`, with `array a=( p q )`:

| input | brace-less | braced |
|---|---|---|
| `echo $a[0]:-z` | `p:-z` | `p` |
| `echo $a[9]:-z` | `:-z` | `z` |
| `echo x $a[9]:~ y` | `x :~ y` | `x y` |
| `echo $a[9]:?m` | `:?m`, **rc 0** | aborts, `m` on stderr |
| `echo $m[no]:-z` | `:-z` | `z` |

The last two rows are the ones that matter.

**`:?` is the guard operator, and the brace-less spelling silently is not a
guard.** `$a[9]:?m` does not abort. An out-of-range cell reads as empty, the
`:?m` concatenates as literal bytes, and the program continues with the string
`":?m"` — which is truthy, non-empty, and looks nothing like the failure the
author asked for. Someone writing `$a[$i]:?missing` in a check has written a
check that cannot fail.

**`:~` is the elide, and brace-less it emits a literal argument instead of
eliding one** — `echo x $a[9]:~ y` passes three arguments where the braced form
passes two. Under DD#84 that is an argument-count difference, not a cosmetic
one.

This is the same shape as the SB5068 hole that drove the "all paths must agree"
ruling two days ago: a spelling that looks like it invokes a rule and quietly
does not.

## The argument for "yes, a cell takes one"

`spec/arithmetic.sh`'s own section header, on the block that specs the *braced*
cell dispositions (`scx.7`-`scx.16`):

> Direct-form cell dispositions (**DG: a string cell behaves like a string
> variable**).

That principle is why braced cell dispositions exist at all. `AGENTS.md`'s
decision tree then uses this very quotation as its worked example of a stated
principle that must be applied to *all* its consequences rather than narrowed:

> "A string cell behaves like a string variable" settles every operator and
> both contexts — not just the case that was asked about.

If a cell behaves like a string variable, and `$x:-z` is a disposition, then
`$a[0]:-z` is one too. On that reading DD#90's scope list is simply incomplete —
written before cell dispositions existed — rather than deliberately exclusive.

## The argument for "no, and codify it"

DD#90's disambiguation rule is a *reinterpretation* of previously-literal text,
and it was accepted on the grounds that the reinterpreted spellings are ones
nobody writes literally:

> nobody writes `$x:-`/`:?`/`:~` as a literal path

That claim is weaker after a `]`. `$a[0]:-` has a plausible literal reading
(`"$a[0]" + ":-"`), and the bytes before the colon are already a compound
expression rather than a bare name, so the eye has more to track. DD#90 also
already declines the operators whose brace-less form reads badly (`:off:len`,
`:s/re/sub/` stay braced) — the precedent for carving an operand *out* of the
brace-less surface on legibility grounds exists.

If the answer is no, it should be **codified and diagnosed**, not left as a
silent literal. The `:?` row above is the reason: a rejected `$a[9]:?m` is fine,
a silently-not-a-guard `$a[9]:?m` is not.

## Out of scope

**Interpolation is already settled and is not part of this.** DG ruled that
brace-less dispositions forgo interpolation ("err on the side of clarity"), so
`"$a[9]:-z"` → `:-z` is *correct* and should stay. The question is unquoted
position only. (`"$a[9]"` itself is a subscript — that half agrees since
2026-10-03.)

The out-of-range cell reading as empty with rc 0 (`echo "[$a[9]]"` → `[]`,
braced and brace-less alike) is longstanding and is what makes `:-` on a cell
meaningful. It is not in question here.

## Where it would live

The scalar/positional/splat brace-less dispositions are handled by
`Scanner::dollar_or_braceless_disposition` (`sash/src/parse/lexer.rs`), which
`scan_dollar` reaches via `dollar_name_token` **only when
`braceless_subscript` declined**. So a cell never reaches the disposition
scanner today: the subscript desugar fires first and returns a
`Tok::DollarBrace("a[0]")`, and the trailing `:-z` is already a separate word
fragment by then.

Extending it means `braceless_subscript` offering the disposition tail to the
same scanner — i.e. emitting `DollarBrace("a[0]:-z")` — which is exactly the
desugar the braced path already understands (`${a[0]:-z}` works). The braced
reader needs no change either way; this is a lexer-side question about how far
the brace-less token runs.

Rejecting instead is a validator-side diagnostic on the concat shape.

## Prior art in this repo

- `notes/decisions/DD90-braceless-dispositions.md` — the scope list, the
  disambiguation rule, and the braces-still-required cases.
- `notes/decisions/DD80-braced-key-discipline.md`, amendment 2026-10-03 — the
  "all paths must agree" ruling and why a spelling that walks around a rule is
  not acceptable.
- `spec/arithmetic.sh` `scx.7`-`scx.16` — the braced cell dispositions, and the
  principle quoted as their basis.
- `spec/subscript.sh` sections N and O — the three spellings now agreeing on the
  subscript itself.
