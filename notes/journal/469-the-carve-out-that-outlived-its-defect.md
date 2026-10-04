# 469 — the carve-out that outlived its defect

**On `hand-parser`.** DG authorised two language-surface changes — the sigil on
a cell inside arithmetic (`$(( $a[1] ))`), and brace-less dispositions on cells
(`$a[0]:-z`) — and asked me to flag anything underspecified. Neither turned out
to need a new mechanism. One of them turned out to be a guard protecting against
a bug that had been fixed the day before. Probing for the second surfaced a
third thing neither of us was looking for: a string cell imported into
arithmetic had been silently reading **0**.

## The guard that was already unnecessary

`$(( $a[i] ))` was SB4035, and `notes/tasks/arith-bare-subscript-followups.md`
explained why in detail. The arithmetic body is collected as a *reconstructed*
string — logos tokens back to text via `tok_to_text`, re-lexed by
`tokenize_arith` — and in August `$a` and `[i]` were separate tokens, so the
round trip re-spaced the index to `$a[ $i ]` and the re-parse read `a[0]`.
Silently. An attempt to accept the spelling was reverted on 2026-08-21 with the
verdict that a correct fix had to be lexer-level:

> Extend `scan_dollar` to fuse `$name[…balanced…]` into a single token,
> mirroring how `scan_word` fuses bare `a[i]`.

That is a precise description of `braceless_subscript`, which I wrote on
2026-10-03 — for the *word* context, to satisfy DG's "all paths must agree"
amendment to DD#80. It fuses exactly that, and I gave it this, at line 969:

```rust
if self.arith_depth > 0 {
    return None;
}
```

I wrote the guard because the task file said arithmetic was unsafe, and the task
file was right about the compiler as it stood before the function I was writing.
The function removed the hazard in the course of doing something else. I carried
the carve-out forward anyway, with a doc comment restating the August rationale
as if it still held.

Prototyping the removal took one rebuild. `$a[1]` → 11. `$a[$i]` → 11, the exact
case the August revert was about. `$a[i+1]` and `$a[a[0]]` → SB4041, which is
what `${a[i+1]}` says too, so the spelling agrees with its braced twin on the
rejects as well as the accepts. Two specs failed, dsx.1 and dsx.2, both pinning
the old reject, and dsx.1's comment helpfully quoted the obsolete reason.

**A workaround should carry an expiry condition, not a rationale.** "Unsafe
because tokens split" is a fact about a build; "remove this when the subscript
survives as one token" is a fact about a *state*, and the second one would have
tripped the moment I wrote the thing that made it true. I had both halves in my
head on the same day and did not put them together.

## The limit that was already there

DG's second change came with a floated restriction: allow brace-less cell
dispositions "possibly with limits, e.g. only where it's clear where the braces
would be if present, such as when bordered by a space char". He asked whether
that was underspecified.

It is not, and the reason is that the restriction already ships. The brace-less
disposition scanner is handed the position *immediately* after the subject and
returns `None` unless it finds `:`. So `$x:-z` is a disposition and `$x :-z` is
two words — adjacency has been load-bearing since DD#90, and a cell inherits it
with no new code. The operand ends at the same word boundary a scalar's does.
Only `:-`, `:?`, `:~` open one; `$a[0]:/bin` stays literal, as `$x:/bin` does.

Where adjacency collides with something, it already collides *loudly*. In
arithmetic `:` is also the ternary, and `$(( c ? $x:-5 ))` is **SB4020** today
where bash answers −5. I measured the cell form: same SB4020, same fix (a
space). That is the trade I would have proposed if it weren't already made — a
diverging reject costs a keystroke, a diverging accept costs a wrong answer.

So the honest answer to "is this underspecified?" was "no, and the limit you're
describing is the one that's running". Worth saying plainly rather than
inventing a cell-specific rule to demonstrate diligence; a third grammar is
precisely what DD#80's amendment closed.

## What the probing found

`:?` is the guard operator, and brace-less on a cell it was not a guard:

```
array a=( p q )
echo $a[9]:?missing      # printed ":?missing", exit 0
```

The cell read empty, `:?missing` concatenated as literal bytes, and the program
continued holding a non-empty truthy string. A check that could not fail. `:~`
was worse in a quieter way: brace-less it passed an argument where the braced
form passes none, which under DD#84 is an argument-count difference. These are
the SB5068 shape — a spelling that looks like it invokes a rule and does not —
and they turn what DG framed as an ergonomic change into a correctness fix.

## The fourth non-total walker

Probing the string-cell interaction, I wrote what I expected to be a control:

```
array s=( hi )
int n=$(( ${s[0]} ))
```

Bare `s[0]` is SB4030 and has been since 2026-08-19 — DD#77 D1, a string→int
crossing owes a disposition. Scalar `$s` is SB4030. This compiled, and `n` was
**0**.

`check_arith_bare_map_subscript` runs on `walk_arith_subscripts`, which ended:

```rust
_ => {}
```

`ArithExpr::Expansion` — the variant carrying `${…}` — fell into it. So the
check only ever saw bare cells, and the braced spelling, which is the *only* way
to read a map cell at all under model B, had never been checked. Six weeks.

That is the fourth time in roughly as many sessions that a non-total match over
an IR enum has been the bug. The stored memory says so; I read it; I still spent
the first ten minutes looking at the type environment. The walkers are where I
should start, not where I end up.

I made it total, with `Expansion(_) => {}` spelled out and commented, and gave
it the `do not add a wildcard` marker that opts it into the pre-commit totality
gate. Two markers were added this session; the gate is doing real work now.

## The false positive, and not forking the walker

My first fix walked `walk_arith_expansion_roots` and promptly broke three
passing specs: `(strlen ${a[1]})` is not an int operand — it passes a *string*
to an intrinsic whose signature wants one — but that walker descends into
`FuncCall` args. `walk_arith_var_reads`, the bare-name walker, deliberately does
not.

The tempting fix was a private copy of `walk_arith_var_reads` with the filter
changed. A stored memory exists for exactly this, earned twice: a fix lands on
the canonical walker and the private fork silently keeps the bug. And the two
halves here are not independent — they are one rule, "which positions are int
operands", split by spelling. A fork would let them drift.

So I used the pattern `ir.rs` already documents a few hundred lines up: one
private total traversal, two filtered views. `walk_arith_int_operands` yields
`ArithOperand::{Bare,Imported}`; `walk_arith_var_reads` keeps its signature and
becomes a filter over it, so its four callers never knew; the new
`walk_arith_imported_operands` is the other view. The `FuncCall` decision is now
made in exactly one place, which is the property that matters.

## The hint that was false, not dead

Removing the carve-out made me look at the SB4035 message that used to fire:

> `$a[i]` isn't a subscript read in arithmetic — use `a[i]` or `${a[i]}`

I assumed it was now dead and nearly deleted it. It is not: the arm is reached
by a *stray* bracket — `$(( $a[1 ))`, `$(( 1[2] ))` — and those still need a
diagnostic. What had changed was that the sentence was now a lie. Reworded to
describe the actual complaint: a stray bracket, and here are the three spellings
of a cell read.

**A diagnostic can be reachable and wrong at the same time**, and the coverage
database cannot tell me which. It would have shown this line warm either way.

## The thing I thought I'd discovered

Measuring the disposition table, I found `int u=0; ${u:-5}` yields `5` where
bash yields `0`, and raised it to DG as a possible divergence. He answered that
bash has a concept of unset variables which sash does not adopt, and that `:-`
targets the additive identity.

This is DD#65 §1. It is journal 209. It was decided in July, it is written down
in the file whose whole subject is the disposition family, and it says, in
those words, "the additive identity, the int analogue of the empty string".

Two sessions ago I retracted a task filing for the same reason — the SB5067
question had been answered in a DD#121 addendum I had not read. I wrote then
that the lesson was to read the decision before filing. Here I didn't file
anything, but I did spend DG's attention asking him to re-rule on something he
had already ruled on, and I briefly carried it in my head as an undocumented
accident that might need a new DD. **"Check `notes/decisions/` first" is not a
rule about filing. It applies to every sentence that starts "I found a
behaviour".** The decisions directory is ~120 files; grepping it for the
operator in hand costs seconds.

DD#123 restates the rule in one short section and points at DD#65, because a
reader arriving at the cell work will ask, and because the restatement is now
load-bearing for *why* `${a[9]:-z}` on a missing cell does what it does.

## Shape of the change

Three changes, one rule, and DG supplied the rule himself in a spec-file header
I had written down months ago and then applied to only the case in front of me:
**a string cell behaves like a string variable.** Every spelling reads, every
spelling disposes, every spelling owes the crossing.

- `braceless_subscript` lost the `arith_depth` guard and gained the trailing
  disposition. That is both features; everything else was consequence.
- `walk_arith_subscripts` is total. `walk_arith_int_operands` is the new single
  traversal, with two views.
- `check_arith_expansion_string_cell` is DD#77 D1's missing imported half.
- `bare_map_subscript_error` is now type-aware, because sending a string map's
  author to `${m[k]}` would be sending them to something that also rejects.

Six existing specs (asx.1–asx.6) asserted the old string-cell reading and were
re-pointed, each keeping its original subject. 22 new specs. Full sweep
**7609**, predicted exactly; `mk test`, fmt, clippy, all four spec-integrity
checks clean.

## What I'd take from this

The three findings have the same shape from different directions. The arith
carve-out was a true statement about August preserved into October. The task
file's "a correct fix is lexer-level" was a true statement that *came true* and
nobody checked. The additive-identity rule was a true statement, written down,
in the right file, that I re-derived from behaviour instead of reading.

In each case the information existed and the cost was in not asking whether it
was still — or already — the case. The mechanical version of that, which is
cheap: when you write a guard, write what would retire it; when you read a note
older than a month, re-measure before trusting it; when you find a behaviour,
grep the decisions before narrating it.
