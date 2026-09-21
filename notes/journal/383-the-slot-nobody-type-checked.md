# 383 — The slot nobody type-checked

A cold-line target in `tok_to_text` led, by three or four steps of "what input
reaches this?", to a compiler ICE and six silently-wrong answers. None of it
was where I started looking.

## How it started

`tok_to_text` (`parse/arith.rs:1523`) had eight cold lines. It rebuilds source
text for arithmetic content collection, and four of the cold arms —
`LParen`, `RParen`, `DblParen`, `DblParenClose` — turned out to be **dead**:
its only caller, `step_arith_collect`, handles those four token kinds itself
before reaching the default case. Genuinely unreachable, and worth recording
as such.

The live cold arms were `DollarParen`, `SingleQuoted`, `DoubleQuoted` and
`Comment`. `DoubleQuoted` looked alarming — it discards its content and
returns the literal three-dot text `"..."` — so I went looking for a program
where a double-quoted string sits inside `(( … ))`.

The two candidates were a map subscript (`m["abc"]`) and a **DD#58 sexpr
argument** (`(strlen "abc")`). Both are rejected, so the discard is harmless.
But probing the second one is what opened the real hole.

## The probe that went wrong in an interesting way

`(( n = (strlen s) ))` with `s="hello"` printed **1**.

I spent a few minutes believing I had found a serious bug before checking how
the *specs* spell it: every one of them writes `$(strlen $s)`, with a `$`.
DD#102 gave `strlen` a **value** operand, so bare `s` is the one-character
string `"s"` and `1` is the right answer. Not a bug, and the doc comment on
`reconstruct_intrinsic_command` says so explicitly.

Worth naming as a method failure: I read the *implementation* first and the
*specs* second. Had I gone the other way the spelling would have been obvious
immediately. The specs are the language definition; the code is one
implementation of it.

## The real finding

Being wrong about that is what made me build the full matrix instead of
testing one case — every operand spelling, in both the `$(…)` command form
and the `$(( (…) ))` sexpr form. Seven rows disagreed:

| spelling | `$(…)` | `$(( (…) ))` |
|---|---|---|
| `len s` (str) | SB4030 | 5 |
| `len $s` (str) | SB4030 | 5 |
| `len n` (int) | SB4030 | **native ICE** / uncompilable C |
| `len $a` (array) | SB5064 | 3 |
| `strlen $a` (array) | SB5064 | **0** |
| `strlen $m` (map) | SB5064 | **0** |
| `strlen $n` (int) | SB4030 | 2 |

One root cause. `check_len_operand` and `check_string_intrinsic_operands` take
a `&Command`, so they only ever see real command statements. The transform
*does* reconstruct a `Command` for the sexpr form — `reconstruct_intrinsic_
command` — but the transform runs **after** type check. The DD#58 operand slot
was the one place in the language where these rules did not apply.

`strlen $a` → `0` is the sharpest of them. The DD#75 task file says, of the
command form, that shipping the check in the same commit mattered because
"without it `strlen a` silently answered `0`, which is the exact defect the
split exists to remove." It still answered 0 — one spelling over.

And `(len n)` on an int was not a wrong answer but a **crash**: native panics
with `ICE: expected string var, got int`, while `--emit-c` emits C that
references an undeclared `n_len` and fails to compile. Two backends, two
different non-answers. The differential oracle earning its keep.

## Two specs had locked the bug in

`58.12` and `58.15` both asserted `(len s)` on a string → byte length: the
polymorphic `len` that DD#75 stage 2 retired. Meanwhile `convert.sh:560`'s
comment says `$(len s)` on a scalar "does not even compile". The suite
contradicted itself, and had since stage 2 migrated the command-form sites
without noticing the sexpr ones.

This is why a green suite is not the same as a correct one. Both spellings
were pinned; only one was right.

Incidentally, `58.14`/`58.15`/`58.16` were **byte-identical** restatements of
`58.10`/`58.12`/`58.13`. Three specs that could never fail independently. DG
chose to repurpose the ids rather than retire them, so they now pin the three
rules that were missing.

## The fix, and why it is small

Two edits, both sharing rather than copying:

- **type_check** reconstructs the sexpr call with the transform's own
  `reconstruct_intrinsic_command` and runs the same two operand checks. Not a
  reimplementation of "what shapes are valid operands" — the same function the
  transform uses, so the two cannot drift.
- **validate** extends SB5064 to arith `FuncCall` arguments, walking via
  `ArithExpr::children()` so a new variant cannot fall silently out of the
  traversal.

The second one generalised further than the intrinsics: `(f $a)` on a *user*
`-> int` function had the same hole, and `$a` on a collection is wrong
wherever it appears, so the rule is on the argument slot rather than on the
callee name.

Ordinary arithmetic was never affected — `$(( $a + 1 ))` has always been
SB4030. It really was only the call-argument slot.

## Result

All twelve differential rows now agree, in both backends. Sweep 5855, zero
failing. Specs `58.12` (migrated), `58.14`–`58.16` (repurposed),
`sx.32`–`sx.35` (new).

Second time this week the shape of the fix was "one rule, two consumers"
rather than "two rules that ought to match" — SB5084 yesterday, this today.
When a check and its subject live in different passes, the question to ask is
which pass owns the rule, not which pass can see the data.
