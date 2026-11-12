title: "bug: `$(int x=1)` is reported as an unterminated `$(`"
status: done
completed: 2026-09-28
priority: P3
kind: bug
depends: []

A declaration inside a command substitution is rejected — correctly — but
with a diagnostic that describes a different problem:

```
$ echo '/bin/echo $(int x=1)' | sash /dev/stdin
error [SB4057] unterminated `$(` — it is never closed before the end of
the source, so everything after it was read as part of it
```

The `)` is right there. What actually happens: `collect_cmd_sub_body`
hands the body to `parse_statement`, which parses `int x=1` as a
declaration and consumes through the `)` looking for the rest of it. The
loop then runs out of input with `closed` still `false`, and the caller
turns that into `unterminated_cmd_sub_diag`.

Verified pre-existing on the unquoted path at `8f7052a`, before the
quoted/unquoted parser convergence (journal 422). The convergence made the
quoted spelling agree with it rather than compiling silently, which is a
strict improvement, but it also doubled the number of spellings that
produce the wrong message.

Not a soundness issue — the program is rejected either way. It is a
diagnostic-quality issue, and a sharp one, because the message actively
misdirects: it tells the author to go looking for a missing paren.

The fix is presumably to detect a declaration as the first statement of a
`$( )` body and say so ("a declaration is not a command; `$( )` captures
the output of a command"). Worth checking what other statement kinds reach
the same end — `collect_cmd_sub_body`'s `closed == false` exit is the only
route to SB4057, so anything that over-consumes lands here.

A spec should pin whichever message is chosen; there is none today.


## DONE 2026-09-28 — and the filing's diagnosis was half wrong

Two corrections to the analysis above, both found by measuring rather than
reading:

**"A declaration inside a command substitution is rejected — correctly" is
false.** It is perfectly legal and always was: `$(int x=1; echo $x)` compiles
and prints `1`. The `;` stops the declaration loop before the `)` is reached.
The fault only ever appeared when a declaration (or a plain assignment —
`$(x=2)` has it too, which the filing does not mention) was the **last**
statement in the body.

**The cause is DD#71 absorb, not `parse_statement` over-consuming.** `)` is
`Tok::RParen`, and `absorb_text` maps it to a literal `")"` in `AbsorbMode::
Assign`/`Operand` — so `int x=1)` absorbed the substitution's own terminator
into the value word and the body genuinely ran to end of input. SB4057 was
telling the truth about what the parser saw; it was the absorb that was wrong.
(`int y=1)` at *top level* is the same thing in plain sight: SB4030 "cannot
assign string value to int variable `y`", because the value became the string
`1)`.)

There is a second, independent site: the spaced spelling `$(int x=1 )` took the
`)` as the next declarator name and reported SB4014 "expected variable name in
declaration". `parse_declaration_assignments` already breaks on `Tok::RBrace`
for exactly this reason — the comment there describes the identical failure for
`fn f { str x="a" }` — and `Tok::RParen` was simply missing from the same set.

### Fix — a closer beats absorb

Settled by DD#71 itself rather than by a new choice. DD#71 scopes absorb "to
whitespace / **closer**", and `]]` already beats absorb for a `[[ ]]` operand:
`[[ $x == a]] { … }` parses today, while `a]b` absorbs its single `]`. Inside a
`$( )` body, `)` *is* the closer.

`Scanner` gains `cmd_sub_depth`, mirroring the existing `arith_depth` exactly
(`enter_cmd_sub`/`exit_cmd_sub`/`in_cmd_sub`), bracketed around
`collect_cmd_sub_body`. `absorb_one_part` returns "not part of this word" for
`Tok::RParen` when the depth is nonzero, and `Tok::RParen` joins the
declaration break set.

Behaviour outside a body is untouched: `str s=a)b` is still the literal `a)b`
(csc.8), and a `[[ ]]` regex operand still absorbs its parens (csc.9).

### Verification

`csc.1`-`csc.10` in `spec/errors.sh` (dual-mode, plan 277 → 287), every case
cross-checked against bash including subshell isolation (`$(x=2)` does not
escape). Two disjoint neuters: removing the absorb guard fails csc.1/3/4/7,
removing the break-set entry fails csc.1/2/3/4. csc.5/6/8/9/10 are regression
guards, not discriminators — csc.10 in particular pins that a genuinely
unterminated `$(` is still SB4057.

Sweep 6683, predicted exactly; zero failing, zero flips.

### Found en route

A **P1 taint escape** — `check_taint` does not walk statements inside a `$( )`
body at all, so `$(str h=$(getenv T); echo x > $h)` compiled clean and let an
environment variable choose a filename. Filed as
`bug-taint-escape-in-cmdsub-body`. Making these bodies parse is what made the
gap reachable enough to probe.
