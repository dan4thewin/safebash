---
title: DD#95 — mixing `&&` and `||` in one list is rejected
status: accepted
---

Settled with DG (2026-09-08), from his question: *"do `!`, `&&` and `||` behave
identically outside `[[ ]]` as within?"*

## The defect

They do not. Everything else matches — `!` (both spellings and its binding),
pure `&&` chains, pure `||` chains, and the statement-level consume rules are
all identical — but a **mixed** chain groups differently:

```
a=x; b=; c=
[[ -n $a || -n $b && -n $c ]]                 # TRUE  — a || (b && c)
[[ -n $a ]] || [[ -n $b ]] && [[ -n $c ]]     # FALSE — (a || b) && c
```

Inside `[[ ]]`, `&&` binds tighter (`parse_test_or` → `parse_test_and`).
Outside, `&&`/`||` are equal-precedence and left-associative. The truth tables
diverge in 2 of 8 rows — whenever `a` is true and `c` is false.

So the **same text meant two different things** depending on where it sat.

## What the other languages do

| language | `&&` vs `\|\|` | bare mixing |
|---|---|---|
| C, C++, Java, C#, JS, Swift | `&&` tighter | allowed — but `-Wall` **warns** ("suggest parentheses around '&&' within '\|\|'"); `-Werror` makes it fatal |
| Go, Rust | `&&` tighter | allowed |
| Python | `and` tighter than `or` | allowed |
| **Ada** | **equal — no relative precedence** | **illegal without parentheses** |
| **POSIX/bash** | **equal, left-associative** | allowed |
| Perl, Ruby | two tiers: `&&`/`\|\|` tight, `and`/`or` loose | allowed |

Three things follow:

1. **sash's inside is the universal convention** — every mainstream expression
   language binds `&&` tighter. Nothing there needed changing.
2. **Shell is the outlier**, and the outside inherited it. The divergence was
   never sash's invention; it is shell-vs-everyone, and sash contains both.
3. **C does not trust its own precedence.** The most precedence-confident
   language on the list warns by default on exactly this shape. "We defined a
   precedence" is demonstrably not enough for readers.

Ada — the safety-critical language — resolves it by making the mixture a
**syntax error**. That is the precedent sash follows.

The deepest lesson is Perl's, because it matches sash's *shape*: Perl and Ruby
have the same two tiers (an expression tier and a control-flow tier) and
distinguish them by **spelling** (`&&`/`||` vs `and`/`or`), not by giving one
spelling two precedences. sash had the two tiers spelled identically; that —
not the precedence — was the real defect.

## The rule

**`&&` and `||` may not be mixed in one list.** SB4053, with advice that
differs by context (DG):

- inside `[[ ]]` → **use parentheses**: `[[ ( a || b ) && c ]]` or
  `[[ a || ( b && c ) ]]`;
- outside → **use `if … { } else { }` or the dispatch form**. There is no
  grouping construct for command lists, and none is wanted: a list that needs
  grouping is asking to be an `if` or a dispatch.

**Pure chains are unaffected**, and that is exactly the minimal rule: `a && b
&& c` and `a || b || c` group the same under either associativity, so nothing
else needs restricting. `!` is unaffected — unary, binds tightest, verified
identical in both contexts. The error-handling idiom (`cmd || die`,
`cmd && next`) is untouched.

**Conditional blocks are not grouping (DG, 2026-11-09).** The accepted
shorthands are `cmd && { a; b }` for `if cmd { a; b }` and
`cmd || { a; b }` for `if ! cmd { a; b }` (DD70). General command
groups are retired; implementation is pending in `retire-command-groups`.
A body has its own lists, just like an explicit `if`, so
`cmd && { other || recover }` does not mix operators in one list.
An actually mixed outer list still gets SB4053. This is not a way to
parenthesize a command list or supply a group's status to an operator.

## What this buys

The inconsistency is **removed**, not papered over: after this, no text means
two things depending on context. The ambiguous form is illegal in one context
and conventional in the other.

A second benefit: it settles the missing-grouping question outside `[[ ]]`.
`( … )` is SB4014 and `{ …; }` is SB4027 there — under this rule that is not a
gap to fill but the point, so no grouping syntax needs inventing.

## Cost

21 sites in the whole corpus, 20 of them one idiom (`X && echo Y || echo N`),
and `contrib/*.sash` had none. All were migrated to `if … { } else { }`.

Three specs were **superseded rather than migrated** — `wordsplit.sh` 62.7,
62.9 and 62.10 existed to pin the shell left-associativity that this DD
removes. They now assert SB4053, with a comment recording what they used to
say.

## Not done

Aligning the two precedences (making the outside bind `&&` tighter), or giving
the tiers separate spellings à la Perl. Either remains open; this DD composes
with both, because it only removes the ambiguous case.

## Addendum (2026-10-31): `&&` and `||` inside `[[ ]]` short-circuit

F8 of the batch-531 port (journal 550; findings B87, B88a). DG's question
was whether `&&` and `||` behave identically inside `[[ ]]` and outside.
The rule above settles how a mixed chain groups. This addendum settles
when an operand runs, which was a second divergence the DD did not list.

Inside `[[ ]]`, as outside, a junction's right operand runs only when the
left one has not decided the result. `[[ L && R ]]` skips R when L is
false, and `[[ L || R ]]` skips R when L is true. A skipped operand is not
evaluated at all:

- a capture in R does not run, so it produces no output and writes no
  global;
- an expansion that would abort is not evaluated. Examples are
  `${y:?…}`, an `argv` index out of bounds, and a zero divisor.

Until this date both sides were eager. The transform lifted every capture
in the test ahead of a single `Test` instruction. Both backends then
evaluated both operands and combined them bitwise (native `band`/`bor`,
C `&`/`|`). So `[[ -n $z && $(f) == y ]]` ran f when `$z` was empty, and
`[[ $x == a || ${y:?} == b ]]` aborted even though x was a.

The junction adds no status rule of its own. The operand that ran last
sets both the result and `$?`, and a `!` over a junction flips both
(DD#70).

Two layers carry this, and each pins what the other cannot:

- **Transform.** A junction in a `[[ ]]` statement or condition becomes
  its own blocks (`test.and.right`/`.merge`, `test.or.right`/`.merge`),
  so a capture runs inside its own operand's block. Specs: lazy.4 and
  lazy.6.
- **Backends.** A junction that stays inside one `Test` is evaluated
  lazily by both backends. This is the only layer an attest predicate
  gets, because an attest keeps its predicate as one instruction (DD#93).
  Specs: atlazy.2, .3, .4 and .7.

Two choices here are decide-and-report:

- `--trace` now prints the junction branches, e.g.
  `+ branch → test.or.merge` (DD#32's rule that every branch traces;
  tr.5).
- lazy.2 and lazy.9 quote their int operands (D1, DD#30), and their ids
  are kept.
