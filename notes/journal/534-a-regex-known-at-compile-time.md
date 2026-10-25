# 534 - A regex known at compile time

2026-10-25. F15 is the third family ported from `taint-walker-redesign`
back to `hand-parser`, on top of F14 (`caf4c61`). Three of its nine
findings port as the branch wrote them:

- B69 and B52, which overlap. SB5023 checked a static regex only in a lone
  `[[ $x =~ re ]]` and in the first edit of an `x =~ s/…/…/` statement,
  where it skipped any RE containing `$`. Every other static pattern
  compiled and failed at run time. One constructor,
  `validate.rs::check_static_regex`, now checks every static pattern:
  - the operands of `sub`, `gsub`, `match` and `matchall`;
  - every edit of every sed form;
  - `=~` under `&&`, `||` and `!`, attest predicates included;
  - every arm of a regex dispatch.
- B51. When a dynamic pattern failed to compile, `sub` and `gsub` returned
  the subject with status 0. They now exit 2, as `match`, `matchall` and
  `[[ =~ ]]` already did.

The other six findings (B62–B65, B67 and B68) are the sed family: they
concern where a `${…:s/…}` payload ends. The branch never fixed them.

## The discussion

On base, three programs misbehaved:

- `echo ${x:s/a)/1/}` printed the subject, reported the error on stderr,
  and exited 0;
- an `=~` under `&&` compiled, then exited 2 at run time;
- `$(sub abc "$p" z)` with a bad dynamic pattern returned `abc`, with
  status 0.

The DDs already settle what should happen:

- DD#39 says a literal pattern "cannot fail at runtime", and that a
  dynamic one aborts with exit 2.
- DD#71 says "Validate-when-static / abort-when-dynamic".

The branch had the code, but no specs and no DD text.

I recommended porting the three findings as-is, with specs and a DD#39
addendum. I also proposed a design for the sed family. DG: *"Do I
understand this next batch of work fixed things not already addressed in
the branch? In the interest of finishing porting the branch code, we
should put anything not already fixed and not blocking the rest of the plan
into the backlog."* Nothing later in the order needs the sed family, so it
went to the backlog.

## What changed

- `check_static_regex` has three callers:
  - `check_regex_literal`, which walks `[[ ]]` and now recurses through
    `!`, `&&` and `||`;
  - `check_static_regex_operand`, for a builtin's operand;
  - `check_dispatch_regex_patterns`, for each dispatch arm.

  The sed sugar, the cell sugar and the statement form all desugar to
  `sub`/`gsub`, so the operand check covers them.
- The parser's first-edit check, `validate_static_sed_re`, is gone. So are
  its helper `split_sed_fields` and its unit test. It raised SB5023 as a
  parse error, which hid validate's other diagnostics; those now appear
  beside SB5023.
- `sash_regex_replace` (sash-rt) exits 2 when a pattern fails to compile.
- `str y=$(matchall "$x" "(")` changes from SB4045 to SB5023. The arity
  error came from type_check, which runs only once validate passes.

Where SB5023 is reported is unchanged from the branch and base:

- an operand or a dispatch arm reports its pattern;
- a `[[ ]]` reports its `[[`, or `attest` in an attest predicate;
- the sugar reports its word, and the statement form its statement.

Whether to report the pattern everywhere is a backlog question.

DD#39's addendum (2026-10-25) states the rule, and supersedes DD#39's
2026-07-08 scope sentence.

## Specs and proof

regex.sh gains 21 dual-mode specs, 39.36–39.56 (plan 93 → 114):

- 15 rejections, each pinning the code, the message and the position;
- 2 positives: a `$` or `^` anchor compiles in every form, and a dynamic
  pattern is left to the runtime;
- 4 runtime exits for a dynamic pattern, one of them under `-`.

No existing spec changes.

Each of four reversion stubs turned exactly its own specs red:

| Stub | Reverts | Red specs |
|---|---|---|
| R1 | the runtime exit | 39.53–39.56, both modes |
| R2 | the recursion through `!`, `&&` and `||` | 39.49–39.52 |
| R3 | the operand check | 39.36–39.44, and the older 39.5 |
| R4 | the dispatch check | 39.47–39.48 |

R3 turning 39.5 red shows that the operand check is now the only thing
that catches the statement form. The base binary fails the 19 specs that
pin a fix, in both modes.

## Gates

- `mk -p 1 test`: 225/27/20. One unit test was removed with the parser
  check.
- One full sweep at `caf4c61`, with F15 uncommitted:
  - 9525 = 9512 passing + 13 skips, which is F14's 9483 plus 42;
  - 42 first-ever ids, no flips and no retirements;
  - the can-fail query reports 260.
- The SB census is empty in both directions. `plan` matches `chk`, and
  both collision checks are empty.
- The pre-commit hook passes, with no bypass.

## The ledger

F15's 8 hunk rows are ported: H010, H095, H127's F15 half, H128, H138,
H249, H251 and H252. The ledger now stands at 70 ported, 1 reimplemented,
194 pending, 64 superseded and 1 drop-proposed. The findings index ticks
B51, B52 and B69.

## Backlog

Eight tasks, under DG's rule:

- `bug-sed-expansion-extent` (P2; B62–B65, B67, B68). Five scanners decide
  where a `${…:s/…}` payload ends, and only the statement form's follows
  DD#63 and DD#96. It also holds the `$'…'` question and the
  `s_malformed` audit that journal 533 promised.
- `bug-subscript-key-check-skips-positions` (P1). DD#80's key-type check
  misses most positions. A word index ICEs natively and silently reads
  index 0 in C.
- `bug-arith-braced-subscript-index-second-grammar` (P2).
- `bug-arith-operator-expansion-unconverted` (P2).
- `bug-sed-index-capture-advice` (P3).
- `bug-sed-sugar-on-int-escapes-dd105` (P3).
- `q-dynamic-regcomp-levers` (P3). Neither `-` nor `if` nor `||` catches a
  dynamic regcomp failure, though DD#55 and DD#70 (A) say they do.
- `q-sb5023-names-the-test-not-the-pattern` (P3).

The five bugs predate the port, and behave the same on base and on the
branch.

## Next

D1 is next: a whole-int `[[ ]]` operand becomes SB4030, per DD#30 (B113).
It waits for DG's go. The P1 above is the candidate to take first, because
it is the only one of the eight that crashes the compiler.
