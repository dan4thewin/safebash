# 533 - A `$` that names nothing

2026-10-24. F14 is the second family ported from `taint-walker-redesign`
back to `hand-parser`, on top of F13 (`68680d8`). Its three findings port
as the branch wrote them:

- B70: a `$` that starts no variable read the variable with the empty
  name, both in arithmetic and as a braced expansion with no name.
- B39: an explicit index that failed to parse was reported as SB4041's
  ambiguous index, whose advice sent the author back to the form they had
  written.
- B66: SB4032's message for an unsupported sed flag carried three runs of
  eighteen spaces.

There are 19 new specs, each run in both modes. Five existing specs keep
their ids under new codes. One new spec was deleted before it landed, as
a rival of an old one. Step 0 recorded B38 as fixed on the branch, and it
was not. It and its siblings B40 and B41 became five backlog tasks, and two
bugs found along the way became two more.

## The discussion

On base, in both backends unless noted:

- `echo $(( $ + 1 ))` prints 1, `echo $(( $- + 1 ))` prints -1, and
  `(( x = $ ))` assigns 0. After every `$` that was neither `${` nor a
  special, the arithmetic lexer scanned a name. When none followed, it
  scanned an empty one, and read the variable with the empty name, which
  is 0. sash has no `$-`, so that one is the empty read followed by
  `- +1`.
- `${}` prints nothing, `${:-d}` prints `d`, and `${:?m}` compiles and
  aborts at run time. `${:0:1}` is a native ICE ("no codegen slot",
  `codegen/types.rs:72`) and prints nothing in C. `${:~}`, `${:!}` and
  `${:s/a/b/}` read an empty value natively, and emit C that gcc rejects.
  Each one expands the variable whose name is empty.
- Inside arithmetic, `$(( ${} + 1 ))` and `$(( ${:-1} ))` print 1.
- As an index, `${a[$(( $ ))]}` reads `a[0]` natively and fails gcc in C.
  `${a[${}]}` is SB4030, "non-integer index on indexed array 'a' — `$` is
  not int-typed…": a complaint about a name nobody wrote.
- `${a[$(( 1 + ))]}` is SB4041: "ambiguous index … Quote a literal key
  (`["…"]`), use a variable (`[$name]`), or open arithmetic explicitly
  (`[$(( … ))]`)". The author had opened arithmetic explicitly; the real
  error is the missing operand.
- `x =~ s/a/b/z` is SB4032. Its message has eighteen spaces inside
  "replace every match", inside "no case-insensitive mode" and inside
  "on both sides for `i`".

Three DDs already point the way:

- DD#72's "Also fixed" made unknown arithmetic bytes a clean reject
  ("zero-tolerance: no silent arith miscompiles"), and a `$` reached the
  same silent zero by another route.
- DD#65's SB4047 sentinel rests on DG's *"need to prevent ambiguity — if
  the compiler doesn't know what something does, that's very bad"*, and
  `${}` builds an expansion on a name nobody wrote.
- DD#80 defines the explicit index forms. Separately, DD#123 had already
  made a stray bracket SB4035.

The branch makes a `$` that starts no variable SB4035, "stray `$` in
arithmetic", and gives `$(` its own SB4035 message. An empty or `:`-led
braced body becomes SB4047, "names no variable". An explicit index reports
its own parse error, and the message's line continuations are restored.
It re-codes 31.46, 31.67, 31.85, cov.356 and 75.164. What it lacked:

- no spec for any of B70's silent cases, only re-codes of programs that
  base already rejected;
- no spec for B66, and none for B39 beyond 75.164;
- the text behind two citations. 75.164 cited a DD#80 addendum dated
  2026-10-20 that was never written. 31.67's comment cited DD#72 for a
  `$` that "opens no variable" being a stray byte, and DD#72 said no such
  thing.

The new `$(` message names three routes:

- capture into a variable first (a string capture converts with a
  disposition, `${s:-0}`);
- call an int function as `(f)` (DD#58);
- group with plain `( … )`.

Before recommending the message, I compiled each route, using a user
function's capture for the first. All three work, so the message sends
nobody to a dead end. I recommended all three findings as written, with the
specs the branch lacked and three DD addenda (DD#65, DD#72, DD#80), and
listed what I would decide and report. DG: *"as-is ok"*.

One decide-and-report item was the ids of the five re-coded specs. The
plan gives a changed spec a fresh id, but that rule exists for specs we
refine or write ourselves. These five are the branch's versions verbatim,
with the programs unchanged, and each one's history is pass then pass.
Keeping the ids hides no flip, so they keep them.

One consequence was not in the message. `${&}` has an empty name run
followed by an unknown byte, so it reaches the same sentinel and the same
renderer as `${}`. Base said "unrecognized content `&` after ``". It now
says "`${…}` names no variable". That is the branch's behaviour, ported
as-is, and vm.nn8 pins it.

DG's rule from F13 governs the rest: *"behaviors that are not obviously
bugs should remain unchanged; questions and uncovered cases need to be
added as backlog tasks"*. `${10}`, `$-` in a word, and B40/B41's reserved
names in arithmetic are not fixed on the branch either. They are tasks,
not digressions (see "Backlog" below).

## A stray `$` in arithmetic

`scan_arith_dollar` now reads the byte after the `$` before deciding
anything:

- `{` opens the braced scan;
- a special (`#` `?` `$` `!` `@` `*`) is a special variable;
- a name byte starts a name;
- `(` is a nested substitution;
- anything else is a stray `$`.

Only the `$` is consumed. The last two cases become `BadLit` tokens, and
`arith_bad_literal_error` reports them as SB4035, the code DD#123 gave a
stray bracket.

Five existing specs change code; their programs do not:

| Spec | Program | Base | Now |
|---|---|---|---|
| 31.46 | `$(( $(getpid) > 0 ))` | SB4020 | SB4035, "is not an arithmetic operand" |
| 31.85 | `(( x = $(echo 2) + 1 ))` | SB4020 | SB4035, the same |
| cov.356 | `$(( $((1)) + 2 ))` | SB4020 | SB4035, the same, and "group with plain `( … )`" |
| 31.67 | `(( x = 1 $'a' ))` | SB4020 | SB4035, "stray `$`" |
| 75.164 | `${arr[$(( $(f) ))]}` | SB4041 | SB4035, "is not an arithmetic operand" (below) |

Base's SB4020 complained about the token that followed an empty name.
31.46's said "unexpected token after arithmetic expression", which names
neither the `$` nor the form. The silent cases get new specs:
31.173–31.175 (`$(( $ + 1 ))`, `$(( $- + 1 ))` and `(( x = $ ))`), and
80.134 for the same thing in an index. DD#72's addendum states the rule.

## A braced expansion that names nothing

`try_leading_form` now asks `names_no_variable` first. A body that is
empty or starts with `:` returns DD#65's unrecognized-content sentinel. The
parser's `unrecognized_operator_diag` then renders an empty name as SB4047,
"`${…}` names no variable — a braced expansion starts with a variable name
(`${x}`, `${x:-…}`)".

The empty-name spellings that base already rejected keep their own codes:

- `${#}`, `${?}` and `${-}` are SB4034;
- `${[0]}` is SB4047, with its own message;
- `${!}` is SB4003;
- `${@}` and `${*}` are SB5045.

The new specs:

- vm.nn1–nn8, in words; nn4, `${:0:1}`, also asserts that nothing ICEs;
- 31.176–31.177, in arithmetic;
- 80.135, as an index.

The rule is in DD#65's addendum.

## An explicit index reports its own error

`parse_whole_index` re-parses an index as a value. When that parse failed,
the text fell back to a literal, and any literal that is not a clean
bareword is SB4041's ambiguous form. Now an index that opens with `$`, `"`
or `'` (`opens_explicit_index`) returns its own parse error, with any
sub-scanner fatal ranked first by F13's `with_scanner_fatal`. Each spec
puts the index on line 2, except 75.164, which puts it on line 3:

| Spec | Line | Base | Now |
|---|---|---|---|
| 80.131 | `echo "${a[$(( 1 + ))]}"` | SB4041 at 2:6 | SB4020 at 2:11 |
| 80.132 | `a[$(( 1 + ))]=x` | SB4041 at 2:1 | SB4020 at 2:3 |
| 80.133 | `echo "${a["$(( 1 + ))"]}"` | SB4041 at 2:6 | SB4020 at 2:11 |
| 75.164 | `echo "[${arr[$(( $(f) ))]}]"` | SB4041 at 3:6 | SB4035 at 3:14 |

Inside `$(( ))` the positions are coarse by design (F13's `Within`
origin). Each one points at the `$` of `$((`, except 80.133's, which
points at the quote that opens the key. 80.131–80.133 assert SB4020,
"ends where an operand is expected", and that SB4041 does not appear.

Two cases stay SB4041:

- **An index that parses but leaves text behind** is still a mixed form.
  A probe, `${m[$s x]}`, is SB4041 in both modes. The existing 80.4
  (`${a[$i+1]}`) and cov.110 (`$a[$i-]`) pin it.
- **An implicit index that fails to parse.** It has no construct of its
  own to blame, and cov.125 (`$(( ${a[]} ))`) pins it. I had written a new
  spec for this case, 80.136, `${a[(1+2)]}`. The stub that sends every
  failing index down the explicit route turned both 80.136 and cov.125
  red, so 80.136 was a rival of a spec that had been there all along. I
  deleted it before it was ever swept.

Two places said the capture case is SB4041: DD#80's 2026-10-03 addendum
and a comment in `type_check.rs`'s `array_index_error`. Both now say
SB4035; the DD keeps a note on why it changed. The new DD#80 addendum
states the rule and both SB4041 cases.

Journal 532 left one path unmeasured: whether the `?` that carries an
assignment's index error out of `parse_split_subscript_index` can fire. It
can. A probe stub on that line fired on 80.132, an assignment's index,
and R2 below, which closes the explicit route, turns 80.132 red.

## Three runs of spaces

`1f8b699` (2026-09-19, "fix: reject sed substitution flags other than g
(DD#63)") added `build_one_sed_sub`'s message for an unsupported flag as
one source line holding three runs of eighteen spaces. Each run is exactly
what a lost line continuation leaves behind: the space before the `\`,
plus seventeen spaces of the next line's indentation. No spec asserted the
text. The branch splits the literal back over four lines, joined by `\`.
63.fl8 (`x =~ s/a/b/z`) asserts the three phrases above, each of which
spans one of the old runs.

## B38 was never fixed

Step 0 recorded B38 as fixed on the branch, with questions: "scalar-name
checks in arithmetic and expansions; mixed results. Questions on `${10}`
and `$-`". I went back to its probes to write the specs, and none of them
shows a B38 fix:

- One is B70's slice: `echo $(( $ + 1 ))` prints 1 on base and is SB4035
  on the branch.
- The other three are not B38 at all, and get the same verdict on both
  binaries. `IFS=x; echo hi` is SB5009. A function that echoes `$#`,
  called as `f a b`, is SB5043, which the branch only moves from 1:1 to
  the call (B54, F13's). A `keys` fill prints `a`.

So I measured B38's own shapes on three binaries: base, branch and F14,
all native. All three give the same verdict on each:

| Program | base, branch and F14 |
|---|---|
| `echo "[$(( $0 ))]"` | `[]` |
| `echo "[$(( ${0} ))]"` | `[]` |
| `echo "[${0}]"` | SB5017, "$0 is not available in sash — use $(arg 0)" |
| `fn f { echo "[${10}]" }`, called with ten arguments | SB5043, "takes exactly 0 arguments but was called with 10" (base at 1:1, the others at the call) |
| `echo "[${1x}]"` | `[]` |
| `echo "[$-]"` | `[$-]` |

What the table shows:

- `$0` is rejected in a word but reads as empty in arithmetic.
- A function that reads `${10}` gets arity 0, so it cannot be called with
  the ten arguments it needs.
- `${1x}` reads a variable nothing can bind.

B38 is open. Four of its pieces are tasks of their own, and `$0` in
arithmetic joins B40 and B41's reserved names.

## A line half written

Re-measuring the base reproducers that the DD#65 and DD#72 addenda cite
turned up a bug that has nothing to do with F14. With `x` empty, `echo
"[${x:?m}]"` prints `[` and then aborts. The same happens with:

- `echo a "${x:?m}" b`, which prints `a `;
- a division by zero inside `$(( ))` in an echo argument;
- `write`;
- a redirect, which leaves `[` in the file.

`/bin/echo` prints nothing, because every argument is evaluated before the
exec. Both backends lower and write a word one part at a time
(`echo_write_one_arg` natively, `emit_echo_write_arg` in C), so an abort in
a later part runs after the earlier writes. I filed it as
`bug-echo-partial-line-before-abort` (P2). The fix has one question to
settle first: whether two parts can share a scratch buffer. If they can,
writing each part as it is evaluated is what keeps the output correct
today.

## Proving each change

There were six native runs of ten spec files, at 2165 checks each: one
unstubbed, and five with reversion stubs. The ten are the seven F14
touches, plus convert.sh, arith_literals.sh and comments.sh. Three of the
five runs revert two independent changes at once. No spec they turn red
contains both constructs, so each red spec belongs to one half. That split
is argued from the constructs, not measured stub by stub; in the table, a
semicolon separates the halves.

| Run | Reverts | Red specs |
|---|---|---|
| R0 | nothing | none |
| R1 | every failing index takes the explicit route | 80.136, cov.125 |
| R2 | no index takes it | 75.164, 80.131–80.134 |
| R3 | a quoted key is not explicit; B66's message | 80.133; 63.fl8 |
| R4 | `names_no_variable`; every non-variable `$` scans a name, as on base | 31.176–31.177, 80.135, vm.nn1–nn7; 31.46, 31.67, 31.85, 31.173–31.175, 75.164, 80.134, cov.356 |
| R5 | the parser's empty-name arm; `$(` read as a stray `$` | 31.176–31.177, 80.135, vm.nn1–nn8; 31.46, 31.85, 75.164, cov.356 |

vm.nn8 stays green under R4 and turns red only under R5. `${&}` reached
the sentinel by DD#65's older route, so only its message is new. Every
run predates 80.136's deletion.

The base binary is the other control. In the seven files F14 touches, it
fails 51 checks per mode:

- the 19 new specs;
- the 5 re-codes;
- 27 of F13's specs, because base predates F13 (21 in types.sh and 6 in
  functions.sh).

The sweep's 38 first-ever ids are the 19 new specs in two modes, so no
retired id was reused.

The first attempt at R0 had a bug of my own: it called the stub script
with no stub named. The script died on an IndexError before touching a
file, and the driver now refuses an empty list.

## Gates

- `mk -p 1 test`: 226/27/20, unchanged; F14 adds no unit test.
- One full sweep at `68680d8`, with the F14 tree uncommitted:
  - 9483 = 9470 passing + 13 skips (native 4844, C 4626 + 13), rows
    6333518–6343000;
  - that is F13's 9445 plus 38, the 19 new specs in two modes;
  - 38 first-ever ids, no flips, no retirements, and the can-fail query
    reports 260;
  - it is the only sweep at that hash.
- Static checks:
  - the SB census is empty in both directions;
  - `plan` matches `chk` in every file but coverage.sh's known 372/370;
  - both collision checks are empty;
  - the new ids sit above the database's high-water marks (31.172 and
    80.130; `vm.nn` was unused).
- The pre-commit hook passes, with no bypass.
- Spec counts: arithmetic.sh 474 to 479, subscript.sh 128 to 133,
  var_modifiers.sh 168 to 176, regex.sh 92 to 93. lift.3b and 75.163
  change only their comments.

## The ledger

All 14 of F14's rows are ported: H073, H074, H082, H089, H091, H099,
H137, H245, H318–H320, H323, H326 and H330.

- H089, H091 and H099 were split rows whose F13 halves landed in journal
  532; they are now whole. F13's other two split rows, H127 and H205,
  wait on F15 and F5.
- H330 is 75.164's re-code. The branch dated the DD#80 addendum it cites
  2026-10-20 and never wrote it. The addendum now exists, dated
  2026-10-24, and the citation is re-dated to match.

The ledger now stands at 62 ported, 1 reimplemented, 202 pending, 64
superseded and 1 drop-proposed.

The findings index:

- ticks B39, B66 and B70;
- corrects B70's entry. On base a lone `$` was a silent read of the empty
  name, not SB4020; only `$'a'` and `$(` drew SB4020;
- moves B38 from fixed-on-branch+question to open+question, pointing at
  its tasks;
- points B40 and B41 at their task, still open.

Journal 531's "69 fixed on the branch, plus 3 that also need a question
settled" stays as the record of what Step 0 believed.

## Backlog

Seven tasks, under DG's rule:

- `bug-digit-led-names` (P2, B38): `${1x}`, `${1_}` and `$1x` read a
  variable nothing can bind. `${1x:0:1}` is a native ICE, and C fails gcc
  for `:s` and `:~`.
- `bug-multi-digit-positionals` (P2, B38): `${10}` and `${01}` read an
  unbound name inside a function, and four functions disagree about which
  positionals exist. DD#75 has both `$1..$n` and `$1..$9`, so the task
  asks DG to choose between any N and only 1–9; I recommend any N.
- `bug-reserved-names-in-arithmetic` (P2; B40, B41 and B38's `$0`):
  reserved and environment names escape SB5017 and SB5018 outside a word.
  `$(( HOME + 1 ))` prints 1. `len HOME` is a native ICE and prints
  garbage in C. `${HOME[0]}` ICEs in both backends.
- `q-word-dollar-dash` (P3, B38): `$-` in a word is literal text.
- `bug-convert-dollar-ten` (P2, B38): sash-convert reads `$10`, `$01` and
  `$1x` as one name at top level.
- `bug-malformed-index-generic-sb4047` (P3, found probing B39): an
  unclosed quote, paren or `]` in an index gets the generic SB4047.
- `bug-echo-partial-line-before-abort` (P2), above.

## Next

Journal 532's three debts to F14 are paid:

- the DD#80 addendum that 75.164 cites;
- DD#72 text behind the specs that cite it;
- the `?` in an assignment's index, which can fire.

F15 is regex and sed operands:

- B69, a static regex that is not checked at compile time;
- B51, the runtime's regex replace exiting 2 on a null program;
- B52, SB5023 not checked at every command depth;
- the sed family B62–B65, B67 and B68, as one chain grammar;
- an audit of whether `build_sed_or_malformed`'s `None` can be reached.

Its discussion goes to DG before any edit.
