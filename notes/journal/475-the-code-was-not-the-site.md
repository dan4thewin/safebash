# 475 — the code was not the site

Re-measured coverage after the deletion program, which is the first thing the
previous session's own notes said to do: `coverage.db` was measured at
`50270ad` and twelve of its thirty-nine files had moved underneath it,
including the four heaviest ranking targets. A ranking taken then would have
been fiction.

Fresh numbers at `d3cd57f`: **98.16% of lines**, 98.86% of functions, 92.03%
of branches. 474 cold lines by the lcov's count.

## The ranking put my own work at the top

`atok_to_text` led the table with **40 cold lines** — four times the next
entry. That is not a coincidence and it is not a discovery about the compiler:
yesterday I deleted `atok_to_text_spells_every_token`, and that test was the
only thing warming all forty arms.

So the question is whether deleting it was wrong. The classification said
"rival", and a rival is a second answer to a question the spec suite already
settles. The spec suite asserts `SB4020` fifty-seven times. It never once
asserts the **spelling** — which is the entire reason the function exists. Its
own doc comment says so: before it, SB4020 told people

> expected an operand in arithmetic expression, found Amp

for a character they had typed as `&`.

So no, the specs did not cover it, and "rival" was the wrong verdict.

But restoring the test is the wrong repair, because the test could never have
caught the bug it was written for. It calls `atok_to_text` directly and asserts
the string it returns. The bug was that *the call site* formatted the token
with `{:?}` instead of calling the renderer at all. The test passes either way.

A spec cannot be fooled that way: it reads the message the user actually gets.
Probing all forty operator spellings in prefix position, **thirty-two** produce
`found \`X\`` end to end from a one-line program. Those are now specs
31.141–31.172. The remaining eight (`+ - ~ !` are prefix operators, `;` is
trivia, `(` groups, `++`/`--` have their own rule) can never *be* the found
token, and neither can the eight operand-like variants — sixteen arms that
exist because the match is declared total.

## The verification that verified nothing

`parse_assignment`'s `SB4014` arm was also cold, and yesterday I recorded
chk 93.19 as covering it before retiring a test.

`SB4014` is raised at **eleven distinct sites** in `parse/`. chk 93.19 asserts
`$err == *SB4014*`, and the site it actually fires is the `[[ -i 42 ]]` test
parse — a different function in a different file. The spec passes; the arm is
cold; both statements were always true together.

The code is not the site. AGENTS.md already warned that grepping a
diagnostic's *text* proves nothing when the text is built with `format!`; this
is the same error one level up, and cheaper to make, because asserting the
bare code is the suite's dominant idiom. The rule now written down: if more
than one site raises the code, the spec has to assert the distinguishing
message, or it is not cover.

Nothing was lost in this instance — the test I deleted on that reasoning was
about `parse_assignment_lhs`, not the cold arm, so the arm was cold before I
touched anything. The reasoning was still invalid, and it was invalid in a way
that would have hidden a real deletion.

I checked the rest mechanically rather than trusting that the damage stopped
there: of every function called from the thirty deleted tests, exactly three
have cold lines now — `atok_to_text` (fixed above), the `expr_to_string`
family, and one wildcard arm in `classify_attest`.

## What the cold lines are, where I could tell

- **`from_arith`'s five `None` arms are cold by design.** Both callers —
  native `emit_arith_op` and `emit_arith_op_c` — only ever call it from an
  `Add | Sub | Mul` arm, so every other answer is unreachable by
  construction. That is the *cost* of the mechanism DG asked for: the match is
  total so a new `ArithOp` is a compile error, and totality over a set only
  one caller-shape can reach is cold by arithmetic. The lines stay.

- **The `expr_to_string` family is validate-shadowed.** It is called 20891
  times and hits the `Literal` arm every single time. Its other arms need a
  call site that passes a non-literal, and every spelling I tried is rejected
  first: `len "$s"` is SB5062, `len ${a[0]}` / `join "$a"` / `keys "$m"` are
  SB4064, `read "v"` is SB5057. No spec can warm these. That is a question for
  DG rather than a thing to decide quietly, because `expr_to_string` is a
  *diagnostic* renderer and an ICE on the error path is a different proposition
  from an ICE in a transform.

## Gates

Sweep **7751** = 7687 + 64, which is the predicted +2 per spec in a dual-mode
file. 7738 passing, 13 skipped, zero failing, zero regression flips, and
exactly **64 new ids**. Gate self-test 248.
