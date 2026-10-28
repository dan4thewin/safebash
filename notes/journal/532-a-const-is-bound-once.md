# 532 - A const is bound once

2026-10-24. F13 is the first family ported from `taint-walker-redesign`
back to `hand-parser`, on top of Step 0 (`566b6ff`). Its three findings
port as the branch wrote them: B3 and B61, wrong positions and dropped
fatals in re-lexed text, and B54, transform diagnostics at 1:1. DG
approved one refinement. Validate becomes the only place SB4044 is
decided, and the lowering backstop that reported the rest at 1:1 becomes
an invariant. The invariant could not go in while legal programs reached
the backstop. So four bugs found on both binaries while porting
(B116-B119) land here too, with one from the branch's ledger (B58).
56 new specs, each run in both modes.

## The discussion

The protocol opens a family with reproducers on both binaries. On base:

- a statement inside `"$( … )"` on line 3 reports its error at 1:1;
- `${x:s/b/$'\101'/}` on `abc` compiles and prints `a101c`, the escape
  read as its literal characters, and an unterminated `$(` as a sed
  pattern prints `babc`. The sub-scanner's fatal is dropped;
- a glued `#` in `"$(echo a;# c)"` (DD#110's SB4058) becomes a command
  named `#`, reported as SB5011;
- a diagnostic raised in the transform reports 1:1. SB5087 for an
  `f @a` on line 6 is one of them.

The branch fixes the positions by giving the lexer an origin
(`lexer::Origin`). A scanner that re-lexes a substring starts numbering
where the substring sits in the program. `Origin::Within` covers text
the arithmetic lexer rebuilds from tokens, which maps back to nothing, so
it reports the enclosing expression's position, coarse by design. Each
re-lex site ranks its sub-scanner's fatal above whatever the sub-parse
returned (`with_scanner_fatal`), as `parse` does for a whole program.
The transform records the statement it is lowering
(`VersionState::stmt_span`) and reports there (`error_at_stmt`).

The refinement candidate was to stop re-lexing altogether. I measured it
first. Base `expr.rs` parses from `&str` in 47 functions and 781 lines,
against 22 functions and 336 lines on a `Scanner`. That is a rewrite the
size of the parser migration. Capture extents inside `"…"` were already
right on both binaries; the only extent bug is in the sed payload, which
is F15's. I recommended the origins as written.

For B54 I recommended the branch's `stmt_span` plus a refinement. One of
the transform's codes is SB4044: `VersionState::define` reported a write
to a const at 1:1 whenever validate had not. Base's validate.rs
accepted that: "The rarer channels (`read`/`mapfile`/`argv` into a
const) are caught by the spanless backstop in `VersionState::define`, so
no path can ever reach the old `define()` panic." DD#86's SB5072
amendment had set the opposite precedent. It replaced the same backstop
for a `const` in a loop body, because the backstop was spanless, and in
a `while` body fired only as a side effect of a leak. So validate's
SB4044 should cover every channel, and the backstop should become an
ICE. The ICE is orthogonal in the AGENTS.md sense: it sees a name that
lowering is binding, not the AST validate checked.

DG: *"go with recommended"*.

Writing this entry, I found that the refinement reverses half of
journal 204. On 2026-08-28 `const x=1; x=2` reached a `panic!` in
`define`. DG's fork C answered with two halves: a flat validate check
in document order, and a backstop in which "the `panic!` becomes a
spanless SB4044", which "catches the remaining channels". The panic
comes back under DG's later rule that an ICE is a regression guard, not
a defense (AGENTS.md, 2026-10-04). It is safe only because validate now
covers every channel and no legal program reaches it.

## A battery of const writes

Before converting anything, I wrote one program per way a name gets
bound: 45 of them, compiled with `--no-taint` so taint diagnostics could
not mask the answer. On base:

- **Only the backstop saw them, at 1:1.** A `readline` target as a
  statement, as a pipeline stage, in a function, under `&` and in
  `while readline` (reported twice there); a `for` variable over words,
  over `@a`, over a glob and in a function; an attest's re-tag of its
  subject, and DD#93's conversion target.
- **Validate missed one in text order.** `fn f { x=2 }; const x=1;
  echo "$x"` compiled and printed 1 (constw.8), because validate
  collected consts as it walked (B116). A `match` fill in an earlier
  function got past it to lowering the same way (constw.7).
- **False rejections.** A function's `const x` made another function's
  local `x` an SB4044, and a capture's did the same to its sibling
  capture's, because validate's const set was flat and built before
  binding resolution (B118, constw.9-10). A const in a self-tail-calling
  function was rejected, and so was a const bound in an `if` body
  (B117, below).

The glob loop carried a fourth bug (B119). A glob `for` kept its
expansion in a variable with the fixed name `__forglob`. A source global
of that name was clobbered: `__forglob=keep` before a glob loop printed
as `[]` after it (forglob.1). Nested glob loops shared the one list, so
the outer loop ran only once (forglob.2). And `const __forglob` drew a
false SB4044 (forglob.3). DD#125 already says the `__` namespace is not
reserved and every synthetic variable goes through `reserve_temp_name`;
this one did not.

Probing the glob loops also turned up two glob bugs unrelated to F13. A
directory portion (`sub/*`, `./*`, `/etc/*`) matches nothing, although
DD#73 specifies it: both backends call `sash_glob(".", 1, pattern)`. And
`a*b*c` matches only names that contain a literal `*`.

## No digressions

The battery raised questions F13 did not need answered: whether `attest`
may re-tag a const, and what a glob should do about dotfiles, ordering,
directory-open errors and a `*` in a non-final segment. I sent them to
DG in one message, each with a recommendation. He answered with a rule
for the whole port:

> in order to finish porting the existing back without endless
> digressions, behaviors that are not obviously bugs should remain
> unchanged; questions and uncovered cases need to be added as backlog
> tasks

Applied to F13, the obvious bugs that the ICE depends on land here,
because without them the ICE would fire on ordinary programs. Everything
else became a task (under "Backlog" below). An attest re-tag of a const
keeps its verdict, SB4044, and only moves to the statement that makes
it.

## A const is bound once

The ICE claims that lowering never binds a const twice. Four lowering
paths did, on legal programs:

- **Merge phis** (B117). A phi is not a write: each version it joins
  was bound by a statement that was itself checked. `define_merge` binds
  it without the readonly test, at both merge sites: a two-way branch's
  (`insert_phis_two_branch`) and a dispatch's (`insert_one_case_phi`).
  The two-way site is pinned by a const in an `if` body, in an `else`
  body and in an attest condition's `else`, and by one in a function
  called with its output redirected, which reaches the same merge
  (constw.12-15). The dispatch half was found late. One reversion stub
  had covered both sites, and splitting it meant showing the dispatch
  site reachable. It is: a dispatch's merge keeps the default arm's
  versions, so a const bound in a `||` default arm got a phi that looked
  like a second binding. That was a false SB4044 on base and on the
  branch (constw.20-21).
- **The function template** (B117). A function's `fn_` block is now
  lowered as a call with no arguments, with its own tail loop
  (`lower_fn_template`), so a self tail call loops instead of inlining
  the body a second time (constw.11, .16 and .18).
- **The suppressed tail specialization** (B117). With the template
  looping, an int function's suppressed self tail call
  (`return $(- f …)`) still bound its const twice. That specialization
  lowers the body again from inside the first lowering, so it now starts
  from the readonly set the body was entered with
  (`TailLoop::entry_readonly`, constw.19). On base the same program was
  already a false SB4044 at 1:1.
- **A captured function** (B58). `capture_func_into` left the caller's
  tail loop in place. In `fn g { … $(f …) … }`, a `return g` inside `f`
  jumped to g's loop header from inside the capture: base compiles that
  program, and it prints nothing at all, rc 0. It is now SB5037 at the
  call (capcyc.1-2). A self-tail-calling function run as a capture was a
  false SB4044 at 1:1 on base (constw.17). The capture now enters the
  callee's own tail loop.

And `__forglob` goes through `reserve_temp_name` (forglob.1-3).

## Validate decides

`check_const_writes` is now the only place SB4044 is decided. It runs
first in the semantic batch, outside the taint switch, on the program
`bindings::resolve` returns. There every local already carries its
scope's qualified name, so one program-wide set of consts is exact
(B118). It walks twice, collecting every const and then flagging writes,
so a write before the declaration is caught (B116). Each writing
statement gets one SB4044 per const it binds, at that statement, the
innermost one that writes.

The channels are `stmt_bound_names`, total over `StmtKind`: an
assignment, a `for` variable, a command's out-params (`read`,
`readline`, `argv`, and a `keys`, `match` or `matchall` fill), an
attest's re-tag (its subject, or DD#93's SOURCE) and DD#93's conversion
target. A statement-level `(( x++ ))` is an assignment by then, and a
condition's `(( ))` that writes is already SB5007. The command half is
`command_bound_names`, new in this port. On base, the scope collection
and SB5092 each called its two halves; now they and SB4044 share the
one function, so the three cannot disagree about what a command binds.
Two channels stay with type_check: `unset`, and the collection an
`attest … in a` re-tags. Every const is a scalar, and both are SB4030
on a scalar.

`define` now panics, and `defining_a_const_twice_is_an_invariant_failure`
pins it. DD#86's addendum records the rule.

Moving SB4044 from the structural batch to the semantic one changes
which diagnostics come with it. Every one of these programs was rejected
before and still is (native, with `--no-taint`, on both binaries):

| Program | Base | Now |
|---|---|---|
| `keys m x` into a const (constw.6) | SB4030 | SB4044 |
| `fn f { match abc b x; … }`, then `const x=1` (constw.7) | SB4018 1:8, SB4044 1:1 | SB4044 1:8 |
| `const x=1`, then `if (( x = 5 ))` | SB5007, SB4044 | SB5007 |
| `const x=1`, then `if x=$(echo y)` | SB4025, SB4044 | SB4025 |
| `const a=(1 2)`, then `a[0]=5` | SB4064, SB4044 | SB4064 |
| `const x=1`, then `x[0]=2` | SB4044 | SB4044, SB5068 |

SB4044 moved from the structural batch to the head of the semantic
one, which runs only when the structural batch reports nothing. So a
structural failure (SB5007, SB4025, SB4064) now reports alone. constw.7
no longer reaches the transform, so its SB4018, a transform diagnostic,
is gone. And SB4044 can now come with semantic codes, SB5068 here. The
fill's re-code is decide-and-report: a const is a scalar, so SB4030 was
a true type mismatch, but SB4044 names the actual mistake.

## Every re-lex site

The audit at the end mapped every hunk of the port to a reversion stub,
and found that the places that hand a sub-scanner its origin had no
spec of their own. Each now has one (relex.12-21), with the construct on
line 3, so a position relative to the substring cannot pass:

| Spec | Construct, on line 3 | Code | Base | Now |
|---|---|---|---|---|
| relex.12 | quoted brace-less cell, `"$m[$(…)]"` | SB5011 | 1:3 | 3:12 |
| relex.13 | unquoted `${m[$(…)]}` | SB5011 | 1:3 | 3:12 |
| relex.14 | unquoted brace-less `$m[$(…)]` | SB5011 | 1:3 | 3:11 |
| relex.16 | a cell's sed RE, `"${m[k]:s/$(…)/b/}"` | SB5018 | 1:6 | 3:23 |
| relex.17 | leading blanks in an index, `"${m[ $(…)]}"` | SB5011 | 1:3 | 3:14 |
| relex.18 | a chain's second edit, `"${x:s/a/b/,s/$(…)/c/}"` | SB5018 | 1:6 | 3:27 |
| relex.19 | a sed field inside `$(( ))` | SB5011 | 1:1 | 3:7 |
| relex.20 | an assignment's index, `a[$(…)]=x` | SB5011 | 1:3 | 3:5 |
| relex.21 | a `=~` chain continued onto line 3 | SB5018 | 1:6 | 3:12 |

`${@[$'\101']}` (on line 2) re-codes as well (relex.15): base reported
the subscript's SB5031 at 2:6, and the index's own SB4063 now comes
first, at 2:13. The `$(( ))` row is coarse by design, the `Within`
origin: 3:7 is where the arithmetic starts. DD#114's addendum records
the origins, DD#115's the fatal channel, and the task
`subscript-index-nested-span` is closed.

Not every candidate was a gap. The transform's SB5011 is shadowed by
validate's spanned check. A disposition default is never re-lexed: a `$`
in it is SB4070 unless single-quoted. One path was not measured: whether
the `?` now in `parse_split_subscript_index`, an assignment's index, can
fire. It carries an index's parse error, which is B39's subject, so F14
measures it.

## What the looping template broke

Two of my changes regressed programs that base rejected:

- **A self tail call's arity.** Base rejected
  `fn f { if (( $1 > 0 )) { return f }; echo "done $1" }; f 3` (too few
  arguments) and its too-many twin with SB5042 and SB5043 at 1:1, but
  only because the template inlined the tail call as an ordinary call,
  which checks arity. The looping template compiled both, and they ran
  (`done ` and `done 0`). `try_producer_tail_call` now checks the arity
  before jumping back, and reports at the call: 1:33 in the one-line
  form (tailar.2-3), 3:12 in the multi-line tailar.1. Run as a stage,
  base co-fired SB5042 and SB5037 at 1:1; it still co-fires them, now
  at the call (1:33).
  The int tail call has the same gap on base (B56); that is F5's.
- **A function as a stage.** The capture cycle B58 fixes has a pipeline
  twin: `fn f { return g $1 }` with `f … | cat` inside `g`. Base's
  SB5037 for it was an accident of its non-looping template.
  `build_function_pipeline_stage` now takes the caller's tail loop away
  while it lowers the stage, so the recursion is reported, SB5037 at
  1:15 (stgcyc.1). Without that, both backends ICE: native at
  `codegen/types.rs:285` (exits 1), C at `emit_c.rs:4449` (rc 101).
  (Corrected in journal 539: that ICE was B120's, the argument
  `$(( $1 - 1 ))` reading the caller's `$1`. Without the take, a stage
  whose argument does not read `$1` silently runs the recursion DD27
  forbids.)
  Giving the stage its own loop would also work, but that is B21's fix
  and belongs to F2; decide-and-report.

## Proving each change

There are 24 reversion stubs, and each turns red only the specs that
pin it:

| Stub reverts | Red specs |
|---|---|
| the origin | relex.1-11 |
| the fatal channel | relex.7-11 |
| `stmt_span` | tpos.1-5, tailar.1-3, capcyc.1-2 |
| the tail call's span | tailar.1-3 |
| the tail call's arity | tailar.1-3 |
| both merge sites, run before constw.19-21 existed | constw.12-15 |
| the two-way merge | constw.12-15 |
| the dispatch merge | constw.20-21 |
| the template | constw.11, .16, .17, .18 |
| `entry_readonly` | constw.19 |
| B58 | capcyc.1-2, constw.17 |
| the stage route | stgcyc.1 |
| the flat set | constw.9-10 |
| validate's SB4044 | 35.3-35.6, 35.8, constw.1-8 |
| `__forglob` | forglob.1-3 |
| each of the nine origin sites | one of relex.12-21; a sed chain's per-edit origin (`build_sed_sub`) both .18 and .21 |

The base binary is the other control: all 56 new specs fail on it.

The stage-route stub also found a hole in the harness. Without the
`take()` the program ICEs, and `spec/common.sh` keeps none of an ICE's
text in `$err`. The native driver prints `p.sh:ICE: …` and exits 1,
which the stderr filter does not match; in C mode the panic exits 101
and its `ICE:` line is filtered out too. stgcyc.1 asserts its code, so
it went red anyway. A check of only `$rc != 0 && -z $out` would have
passed on the ICE, and on a compiler hang as well. Filed as
`spec-harness-hides-compiler-ice` (P2). How many specs assert only that
shape is not measured.

The first run of the stubs had a bug of my own. Its restore trap used
relative paths after `cd spec`, so every restore failed and the stubs
stacked. I recovered the pristine files from their first backups and
rewrote the driver: absolute paths, a plain `cp` so cargo sees a fresh
mtime and cannot keep a stubbed binary, and a hash of `git diff -- sash`
checked after every restore.

## Gates

- `mk test`: 226/27/20, one unit more than before (the ICE's).
- Three full sweeps at `566b6ff`. The third: 9445 = 9432 passing + 13
  skips (native 4825, C 4607 + 13), rows 6314628-6324072, no flips and
  no retirements. First-ever ids across the three sweeps were 84, 2 and
  26: 112, the 56 specs in two modes. The can-fail query reports 260.
- The first commit attempt failed the hook's complexity gate on
  `lower_function_stmt`: cognitive 5 against a bar of 4.55, three times
  the mean (cyclomatic 7 was under its 10.4). Those were base's scores;
  the template change touched the function, and grandfather-on-modify
  then holds it to the bar. Its arity-defect loop moved, messages
  verbatim, into `report_arity_defects` (5 and 3), which leaves it at 3
  and 2. Plain `mk test` then hit `go-mk-test-deadlock`'s runner
  deadlock before any cargo test ran; `mk -p 1 test` passed 226/27/20.
  A fourth sweep repeated the third exactly: rows 6324073-6333517, no
  flips, no retirements, no first-ever ids, can-fail 260. That hash now
  carries four sweeps, so only the row-order query compares them.
- The SB census is empty in both directions; `plan` matches `chk` in
  every file but coverage.sh's known 372/370; both collision checks are
  empty; the new ids sit above the database's high-water marks.
- Spec counts: errors.sh 465 to 491, functions.sh 214 to 220, types.sh
  409 to 430, glob.sh 14 to 17.

## The ledger

All 48 of F13's rows are done: 47 ported, and H164 reimplemented, since
`define` is now an invariant and validate covers every channel. One
split row, H081, is ported whole: F13 needed its `try_leading_form`
extraction, and its B70 behaviour is H082's, which F14 ports. Five split
rows (H089, H091, H099, H127, H205) have their F13 half ported and stay
pending for the rest. That leaves 216 pending, 64 superseded and 1
drop-proposed. The findings index ticks B3, B54 and B61, adds B116-B119
under F13, and ticks B58 (F5's) as landed here.

## Backlog

Nine tasks, under DG's rule:

- `bug-dispatch-in-boolean-positions` (P2): a dispatch is accepted as an
  `elif` condition and as a `&&`/`||` right operand; DD#66 forbids both.
- `bug-glob-multi-star` (P2) and `bug-glob-directory-portion` (P2),
  above.
- `q-glob-semantics` (P2): dotfiles, order, directory-open errors, and a
  `*` in a non-final segment.
- `q-attest-retag-of-const` (P3).
- `q-empty-keyword-condition` (P3): `if then … fi` and `while do … done`
  compile, and the condition is false.
- `diag-elif-clause-has-no-span` (P3): an `elif` clause's parse errors
  point at the `if`.
- `spec-cov306-308-pin-the-wrong-pass` (P3): those three specs pin
  validate's SB4044, not the transform-time gate they describe.
- `spec-harness-hides-compiler-ice` (P2), above.

## Next

F14: malformed names and arithmetic (B70, B39 and B66, with B38, B40 and
B41). It owes the DD#80 addendum that 75.164 cites, and a ruling on the
B70 specs' citation of DD#72 for "the `$` starts no variable", which no
DD says. Its discussion goes to DG before any edit, under the same rule.
