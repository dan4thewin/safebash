# 453 — the brace that was glued to the word

DG's one-line counterexample opened this, and two of his follow-ups changed
the fix. Both times I was about to ship something narrower than it should
have been, or wider.

## The premise that was false

`bug-brace-breaks-unquoted-word` was filed P2 with an explicit rationale:
"Unlike the `#` case this is **not** silent: it is a compile error, so no
binary disagrees with bash."

DG: "re the P2, seems like this works: echo a{echo z}".

It does. sash printed `a` then `z`; bash prints the literal `a{echo z}`.
`--emit-ir` showed **two statements** — an `echo` command and a `:group`. A
`{` in a command's word list silently ended the list and opened an anonymous
block, no separator required, spacing irrelevant.

I had checked only the error spellings (`a{b,c}` → SB5011, `a{b` → SB4014) and
generalised from them. The silent set is exactly the brace bodies that resolve
as a command: builtins, defined functions, absolute paths. Everything else
errors *incidentally*, because DD#23 rejects a bare name — which is why the
sample I happened to take was all errors.

**Reclassified P1.** A filing's own rationale is evidence like any other, and
it can be wrong.

## The first fix was too wide, and the suite said so

I measured the block's usefulness: it does not scope (`x=inner` leaks out),
there is no `local` at all, and a redirect or a pipe on it is already
SB4021/SB4022 — *"wrap the compound in a function"*. I reported it as inert
and recommended removing the statement-position group, so `{` would only ever
open a body after a header. DG: "yes; file a task for sash-convert too."

The sweep failed 14 checks. `errors.sh` 55.51-55.56 and `functions.sh` 18.8,
all of this shape:

```
/bin/false || { echo a; echo b; }
```

**The group is not inert.** It is the only way to put more than one statement
on the right of `||`/`&&`. I had tested scoping, redirect, and pipe, and never
tested the operator-operand position — so "inert" was a conclusion drawn from
three probes that happened to agree.

The total was right (7121, exactly my prediction), which is what made the
failures legible as breakage rather than bookkeeping. A green sweep whose
total you did not predict is worth nothing; a red one whose total you *did*
predict tells you precisely what you broke.

I stopped and reported rather than rewriting the seven specs to make my chosen
path green. Rewriting them would have been the easy move and it would have
quietly deleted a language feature DG had approved removing only on the
strength of my "inert" claim.

## The second fix was still too wide

The replacement: make `{` terminate a command's word list *only* inside a
control-flow header, and be an error elsewhere. That closes the P1 and keeps
`|| { … }`.

DG: "is there a special error code that rejected local functions that is now
changing? we don't have a local keyword anymore, so wouldn't that error be
appropriate?"

Checked: SB5011 is the **generic** DD#23 unknown-command error, shared with
`resolve.rs` — nothing dedicated was being lost. DD#86 abandoned DD#18's
`local fn` and notes `local` "was the only other user of `local`". So
`local fn inner { … }` correctly reports *"command `local` not found"*.

But it is the same shape as `echo a{echo z}` — a command, some words, a brace.
A position-based rule replaces a useful message about `local` with a complaint
about braces, at parse time, before resolution can speak.

## Adjacency

`a{` is one word. `a {` is two tokens. That line keeps everything:

| | before | after |
|---|---|---|
| `echo a{echo z}` | silent `a`/`z` | **SB4071** |
| `echo a}b` | SB4014 "expected expression" | **SB4072** |
| `local fn inner { … }` | SB5011 | SB5011 |
| `cmd \|\| { a; b; }` | works | works |
| `{ echo z }` | works | works |
| `fn f{…}`, `END{…}` | works | works |

Zero existing specs changed behaviour. sash already uses adjacency exactly
this way for the DD#110 `#` rule, so the mechanism was in the file
(`next_is_adjacent`) before I needed it.

Adjacency alone is not enough, and DG's own program is the proof:

```
END{echo end}fn f{echo f}if (((argc) > 1)){echo yes}else{echo no}f
```

Every `{` there is glued to the previous token. They are fine because none
follows a *command word list* — `END`, `fn f`, `))` and `else` are
distinctive. But `if /bin/true a{echo y}` **is** a command word list, with `a`
as an argument, and the brace must still close the header (DD#68). So the rule
needs both axes: inside a header a `{` always terminates; outside one, an
adjacent `{` is SB4071.

`Scanner::header_depth` mirrors the existing `arith_depth`, set by
`parse_compound_list` — the single choke point every `if`/`elif`/`while`/
`until` condition passes through. Three lines, one place.

## What the third failure taught

SB4072 (stray `}`) broke `dspan.1`-`dspan.3`. Those specs pin that a
diagnostic names its own line and column — `parts_to_expr` once built every
"expected expression" with a hardcoded `Span { line: 1, col: 1 }`.

SB4072 reports the same spans (2:1, 2:12, 2:18), so updating the expected code
preserved their intent. But they reached `parts_to_expr` *via a stray `}`*,
and SB4072 now intercepts that — so the hardcoded-1:1 guard would have been
silently gone while all three specs sat green.

Added `dspan.4` with a stray `)`, which still lands there. **Retargeting a
spec at a new diagnostic can orphan the code it was protecting.** The spec
keeps passing and tells you nothing.

Its first run failed: I computed the column for `echo a)b` and wrote
`/bin/echo a)b`. Caught because the assertion was on the exact column and not
on `-n $err`.

## The residual, on record

`echo a { echo z }` (spaced) is still a command plus a block; bash reads three
literal words. Pinned as `bc.g6` so it is a decision rather than an oversight.
A spaced brace reads as a deliberate block, and a statement-position block is
an existing sash surface bash rejects outright.

## Both diagnostics' advice was executed

`bc.g3` and `bc.g10` compile and run what the messages suggest (`'a{b}'`,
`'a}b'`) and check the output matches bash. The SB5026 sequel two sessions ago
advised `@{g:-x}`, which is itself rejected in that position — a message is
not verified until its advice has been run.

## Numbers

Sweep **7129** (3667 native, 3449 c, 13 skipped) — predicted exactly at each
of four sweeps. `mk test` rc=0. `dispatch_statement` is cc 23 / cog 1, an
exempt dispatcher; `brace_after_word` is 3/2.

Filed `convert-brace-forms-passthrough` (P2): `sash-convert` emits bash brace
forms verbatim, so bash's literal `a{b}` now converts to something SB4071
rejects. The redirect/pipe group rows in it are pre-existing.
