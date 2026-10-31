---
title: "Batch 531 findings: index and port status"
status: open
priority: P1
kind: tracking
depends: []
gates:
  - "every finding below is fixed with a spec, or closed by a recorded decision"
  - "every hunk of the reference branch is ported, re-implemented or dropped by a decision"
---

## What

Coverage batch 531 (the last cold lines in `type_check.rs` and
`validate.rs`) stopped on bugs and, over several sessions, accumulated
about seventy fixes, a rewritten taint walker, one new rejection (SB5098)
and 101 new specs in one unreviewed WIP commit, `e98fca8`, on the local
branch `taint-walker-redesign` (parent `1309385`). DG judged it too large
and lacking the usual narrative. The branch is now a read-only reference.
Its content is ported to `hand-parser` one family at a time, each step
opening with a discussion with DG. Journal 531 records why, the decisions
already settled (D1, the taint model T1-T3) and the order of the steps.

This file is the index: one entry per finding, grouped by the step that
lands it, in landing order. Each entry gives the family (when it differs
from the heading), the status measured on both binaries in both backends
at the start of the port, the governing DD and the branch's specs, if any.
A step ticks its entries when it lands and names its journal.

The branch's code is inventoried in `batch-531-hunks.tsv`, beside this
file: one row per hunk of `git diff 1309385 e98fca8`, with its family
(or per-family line ranges, where one hunk serves several), its findings
and its status. A step marks its rows when it lands. `superseded` marks
the walker's hunks, which track W re-implements rather than ports. At
wrap-up no row may remain `pending` or `drop-proposed`.

Statuses:

- `fixed-on-branch`: the reference branch fixes it; the step ports or
  re-implements the fix, after its discussion.
- `open`: fixed nowhere yet.
- `question`: needs DG's decision first.
- `branch-regression`: introduced by the reference branch; base is
  correct. Not ported.
- `not-a-bug`, `duplicate`, `not-reproduced`, `control`: no work (ticked).

A compound status (`fixed-on-branch+question`) means the branch has a fix
but the step must first settle a question.

The taint walker itself (the branch's context-sensitive walker) is not
ported. Track W re-implements the taint checks under T1-T3, which DD127
now states in full, including DG's 2026-10-31 rule for a function with one
call site. Most of
track W's findings were fixed inside that walker, so W re-implements or
re-probes them; some may vanish by construction, which the step records.

## Index

### 2. F13 re-lexed text positions

- [x] **B3** (fixed-on-branch; DD114, DD115). A statement inside a
  double-quoted capture reports a span relative to the re-lexed substring:
  an error on line 3 reports 1:1 (the branch: 3:21). `command_sub_to_expr`,
  `parse_whole_index` and `parse_interpolating_word` re-lex with a fresh
  scanner.
  - Landed in F13 (journal 532), ported as-is: the lexer's `Origin` is
    threaded to every re-lex site. Specs `relex.1`-`relex.6`, and
    `relex.12`-`relex.21`, one per remaining place that hands a re-parsed
    substring its position (each turns red under a reversion of its site).
- [x] **B54** (fixed-on-branch; no DD). Transform diagnostics report
  position 1:1 (the branch: the real line and column).
  - Landed in F13 (journal 532), ported as-is: `VersionState::stmt_span`
    and `error_at_stmt`. Specs `tpos.1`-`tpos.5`, and `tailar.1`-`tailar.3`
    (a self tail call's arity error, 1:1 on base; F13's template loops, so
    the tail call now checks arity itself, before jumping back).
- [x] **B61** (fixed-on-branch; DD114, DD115). Fatal errors from a
  sub-scanner at a re-lex site are dropped: base compiles and prints `a101c`
  where the branch reports SB4063.
  - Landed in F13 (journal 532), ported as-is (`with_scanner_fatal`).
    Specs `relex.7`-`relex.11`, and `relex.15`: in `${@[$'\101']}` base
    reported SB5031 for the subscript at 2:6; the index's own SB4063 now
    comes first, at 2:13.

Found while porting F13, on both binaries, and fixed with it (journal 532).
The `define` ICE needed all four first: without them, a const write that
validate missed (B116) or a legal program (B117-B119) would ICE:

- [x] **B116** (open; DD86). SB4044 missed writes, or placed them at 1:1.
  Validate walked in textual order and listed only some channels, so a
  write in a function defined before the const compiled (`fn f { x=2 };
  const x=1`), and a `readline` stage or `for` variable reached the
  lowering backstop, which reported 1:1. Validate's `check_const_writes`
  now covers every channel, after binding resolution. Specs
  `constw.1`-`constw.8`.
- [x] **B117** (open; DD86). The lowering backstop raised a false SB4044 on
  legal programs. A const bound inside an `if` or `else` body, an attest
  condition's `else`, a dispatch's default arm, or a function whose output
  is redirected reached a merge phi, which re-`define`d it. (The dispatch
  merge keeps the default arm's versions, so its phi looked like a second
  binding; found late, while auditing hunks against reversion stubs.) A
  self-tail-calling function's `fn_`
  template inlined its tail call instead of looping, binding its const
  twice. Once the template looped, the int tail call's suppressed
  specialization did the same: it lowers the body again from inside the
  first lowering, with the const still bound. Fixed by `define_merge`, a
  looping template and `TailLoop::entry_readonly`.
  Specs `constw.11`-`constw.16`, `constw.18`-`constw.21`.
- [x] **B118** (open; DD86). Validate's const set was flat and built before
  binding resolution, so a function's `const x` made another function's
  local `x`, or a sibling capture's, a false SB4044. Specs `constw.9`,
  `constw.10`.
- [x] **B119** (open; DD125). A glob `for` kept its expansion in the fixed
  name `__forglob`. A source global of that name was clobbered, a nested
  glob loop overwrote the outer loop's list (the outer loop ran once), and
  a source const of that name drew a false SB4044. Now a reserved
  temporary. Specs `forglob.1`-`forglob.3`.

B58 (§12) also landed with F13, because the `define` ICE needs it
(`constw.17`).

Filed as backlog tasks during F13, under DG's rule that a behaviour that is
not obviously a bug stays unchanged in the port:

- `bug-dispatch-in-boolean-positions` (P2): a `[[ … { } ]]` dispatch is
  accepted as an `elif` condition and as a `&&`/`||` right operand, where
  DD66 forbids both.
  - Fixed in F1+F4 (journal 537): SB4028 at the dispatch, in every boolean
    position (cnd.8-cnd.10, cnd.13-cnd.17, cnd.19-cnd.21; DD66 addendum
    2026-10-26).
- `bug-glob-multi-star` (P2): `a*b*c` matches only names containing a
  literal `*`.
- `bug-glob-directory-portion` (P2): `sub/*`, `./*` and `/etc/*` match
  nothing, where DD73 specifies them.
- `q-glob-semantics` (P2): dotfiles, order, directory-open errors, and a
  `*` in a non-final segment.
- `q-attest-retag-of-const` (P3): may `attest` re-tag a const? Still
  SB4044, now at the statement.
- `q-empty-keyword-condition` (P3): `if then … fi` and `while do … done`
  compile and are false.
  - Answered in F1+F4 (journal 537; decide-and-report): SB4027 "this
    header has no condition" (cnd.1-cnd.4; DD68 addendum 2026-10-26). The
    condition was not false: it tested the previous command's status.
- `diag-elif-clause-has-no-span` (P3): an `elif` clause's parse errors
  point at the `if`.
  - Fixed in F1+F4 (journal 537): an `elif` or `else` clause's errors
    point at the clause (skw.26-skw.28, cnd.22).
- `spec-cov306-308-pin-the-wrong-pass` (P3): `cov.306`-`cov.308` pin
  validate's SB4044, not the transform-time gate they describe.
- `spec-harness-hides-compiler-ice` (P2): `spec/common.sh` drops a
  compiler ICE's text from `$err` in both modes (native rc 1, C rc 101),
  so a check asserting only `$rc != 0 && -z $out` passes on an ICE, and
  on a compiler hang.

### 3. F14 malformed names and arithmetic

- [ ] **B38** (open+question; no DD). Invalid parameter names are
  accepted or swallowed in arithmetic and expansions: `$(( $0 ))` and `$((
  ${0} ))` print empty, where the word `${0}` is SB5017. Questions on
  `${10}` and `$-`.
  - Corrected in F14 (journal 533): Step 0 recorded it as
    fixed-on-branch+question, but base, the branch and F14 give the same
    verdict on each of its shapes. The one probe the branch changes,
    `$(( $ + 1 ))`, is B70's. Tracked by `bug-digit-led-names`,
    `bug-multi-digit-positionals`, `q-word-dollar-dash` and
    `bug-convert-dollar-ten`; `$0` in arithmetic by
    `bug-reserved-names-in-arithmetic`.
- [x] **B39** (fixed-on-branch; DD80, DD114). Index parse errors were
  swallowed. The branch re-codes 75.164 from SB4041 to SB4035, citing a DD80
  addendum that was never written.
  - Landed in F14 (journal 533), ported as-is: an index that opens with
    `$`, `"` or `'` reports its own parse error. DD80's addendum, now
    written, states the rule and the two cases that stay SB4041. Specs
    `80.131`-`80.134` and the re-coded `75.164`; `cov.125`, `80.4` and
    `cov.110` pin the cases that stay SB4041.
- [ ] **B40** (open; DD125). Reserved-name reads in arithmetic escape
  SB5031: `handled_elsewhere` exempts environment, rejected and slot names.
  Same root as B41. Tracked by `bug-reserved-names-in-arithmetic`.
- [ ] **B41** (open; DD125). `${HOME[0]}` ICEs in both backends. Same root
  as B40. Tracked by `bug-reserved-names-in-arithmetic`.
- [x] **B66** (fixed-on-branch; no DD). The SB4032 unsupported-flag message
  carries three 18-space runs (lost line continuations).
  - Landed in F14 (journal 533), ported as-is. Spec `63.fl8`.
- [x] **B70** (fixed-on-branch; DD72, DD79). A lone `$` in arithmetic is a
  silent read of the variable with the empty name (`$(( $ + 1 ))` prints
  1); a nested substitution (`$(`, `$((`) or `$'…'` draws the generic
  SB4020. `${}` and `${:…}` expand the empty name too (`${:0:1}` is a
  native ICE). The branch gives SB4035 with a message naming the form,
  re-coding 31.46, 31.67, 31.85 and cov.356, and makes `${}` SB4047. Its
  spec comments attribute "the `$` starts no variable" to DD72, which no DD
  says.
  - Corrected in F14 (journal 533): Step 0 had the lone `$` drawing
    SB4020, as only the nested forms and `$'…'` do.
  - Landed in F14 (journal 533), ported as-is. DD65's and DD72's addenda
    state the two rules; DD72's now says what the spec comments cite.
    Specs `31.173`-`31.177`, `vm.nn1`-`vm.nn8`, `80.134`, `80.135`, and
    the re-coded `31.46`, `31.67`, `31.85` and `cov.356`.

Filed as backlog tasks during F14, under DG's rule that a behaviour that is
not obviously a bug stays unchanged in the port:

- `bug-digit-led-names` (P2, B38): `${1x}`, `${1_}` and `$1x` in
  arithmetic read a variable nothing can bind; `${1x:0:1}` is a native ICE.
- `bug-multi-digit-positionals` (P2, B38): `${10}` and `${01}` read an
  unbound variable inside a function. DD75 has both `$1..$n` and
  `$1..$9`; the task asks which.
- `bug-reserved-names-in-arithmetic` (P2; B40, B41, B38's `$0`): reserved
  and environment names escape SB5017 and SB5018 outside a word.
- `q-word-dollar-dash` (P3, B38): `$-` in a word is literal text.
- `bug-convert-dollar-ten` (P2, B38): sash-convert reads `$10`, `$01` and
  `$1x` as one positional name at top level.
- `bug-malformed-index-generic-sb4047` (P3): an index with an unclosed
  quote, parenthesis or `]` gets the generic SB4047.
- `bug-echo-partial-line-before-abort` (P2): `echo` and `write` print part
  of their line before an argument aborts.

### 4. F15 regex and sed operands

- [x] **B51** (fixed-on-branch; DD39, DD63). The runtime's regex replace on
  a null program exits 2.
  - Landed in F15 (journal 534), ported as-is. DD39's addendum (2026-10-25)
    states the rule. Specs `39.53`-`39.56`.
- [x] **B52** (fixed-on-branch; DD63). Regex SB5023 was not checked at every
  command depth.
  - Landed in F15 (journal 534), ported as-is. DD39's addendum (2026-10-25)
    states the rule. B52 and B69 overlap, and one constructor fixes both.
    Step 0 filed `sub`/`gsub`, the sugar, and `match` under `if` and `-` as
    B52; it filed the dispatch and `- matchall` as B69, whose probe
    directory also holds the `[[ ]]` cases. Specs `39.36`-`39.52` cover
    the two together.
- [ ] **B62** (open; DD63). An unclosed `${x:s/b/c/` compiles and runs; DD63
  says SB4032.
  - Tracked by `bug-sed-expansion-extent`.
- [ ] **B63** (open; DD63, DD96). A `}` in a chained edit closes the scalar
  expansion: `"${x:s/a/c/,s/}/d/}"` draws SB4032 (unquoted, SB4072).
  - Tracked by `bug-sed-expansion-extent`.
- [ ] **B64** (open; DD96). DD96's line break after `,` is rejected in the
  scalar expansion form; the cell and statement forms accept it.
  - Tracked by `bug-sed-expansion-extent`.
- [ ] **B65** (open; DD65). A `"` in a scalar payload stops the scan
  (SB4057); DD65 makes quotes ordinary bytes inside `${…}`.
  - Tracked by `bug-sed-expansion-extent`.
- [ ] **B67** (open; DD63). The cell form `${a[0]:s/}/x/}` truncates at the
  RE's `}` (SB4032); DD63's Path B applies to the cell sugar too.
  - Tracked by `bug-sed-expansion-extent`.
- [ ] **B68** (open; DD63). A positional sed subject with `}` in the RE, `fn
  f { echo ${1:s/}/x/} }`, draws SB4072: the lexer's `sed_prefix_end`
  requires a letter or `_` first, while the parser's `scan_param_name`
  accepts digits.
  - Tracked by `bug-sed-expansion-extent`.
- [x] **B69** (fixed-on-branch; DD39, DD63). A static regex was not checked
  at compile time.
  - Landed in F15 (journal 534), ported as-is. DD39's addendum (2026-10-25)
    states the rule. Specs `39.36`-`39.52`, shared with B52 (see B52).

Filed as backlog tasks during F15, under DG's rule that what the branch did
not fix, and what does not block the rest of the port, goes to the backlog:

- `bug-sed-expansion-extent` (P2; B62-B65, B67, B68): five scanners decide
  where a `${…:s/…}` payload ends; only the statement form's follows DD63
  and DD96. It also covers `$'…'` in a payload (DD63 vs DD65).
- `q-dynamic-regcomp-levers` (P3): DD55 and DD70 (A) say `-` suppresses,
  and `if`/`||` catch, a dynamic regcomp failure; no regex builtin honours
  either lever. DD39's 2026-05-24 clarification says only that it aborts,
  and DD71 names no lever.
- `bug-subscript-key-check-skips-positions` (P1): DD80's key-type check
  runs on a private walker. A word index ICEs natively and reads index 0 in
  C in twelve positions, and fourteen more programs compile.
- `bug-arith-braced-subscript-index-second-grammar` (P2): inside `$(( ))`,
  an operator-free `${a[K]}` parses K with a second index grammar, so legal
  indexes are SB4041.
- `bug-sed-index-capture-advice` (P3): a sed-expansion index on an indexed
  array gets the capture advice, which it cannot follow.
- `bug-arith-operator-expansion-unconverted` (P2): a substring or sed
  expansion imported into arithmetic converts with no disposition (DD77).
- `bug-sed-sugar-on-int-escapes-dd105` (P3): `${n:s/…}` on an int compiles,
  because the desugar writes the `"$n"` conversion.
- `q-sb5023-names-the-test-not-the-pattern` (P3): SB5023 in a `[[ ]]` test
  reports the `[[`, and in the sugar and the statement form the word or the
  statement, so two bad patterns in one test cannot be told apart.

### 5. D1 int operands in `[[ ]]`

- [x] **B113** (open; DD30, DD66). Whole-int operands in `[[ ]]` compile,
  e.g. `[[ $(( 10 / z )) == 1 ]]`. Settled (D1): SB4030 "quote it"; `"$n"`
  and in-word interpolation stay legal.
  - Landed in D1 (journal 535). The rule covers `[[ ]]` tests, dispatch
    subjects and patterns, and attest predicates; sash-convert quotes the
    int operands it writes. DD30, DD66 and DD57 addenda (2026-10-25). Specs
    `30.50`-`30.65` and `57.178`-`57.191`; 15 existing specs rewritten to
    quote. END is not checked yet (see B5).

Filed as backlog tasks during D1, under the same rule as F15's:

- `bug-fresh-clone-rustix-runtime-private` (P1): rustix 1.1.5 made
  `rustix::runtime` private, and no Cargo.lock is tracked, so a fresh clone
  cannot build sash-rt. The remedy is DG's call.
- `bug-readline-into-int-ice` (P2): `readline` into an int ICEs natively and
  emits C that gcc rejects; D1's attest advice leads straight to it.
- `bug-expr-type-int-disposition` (P2): `${n:?msg}` on an int is typed as a
  string, unlike `$n` and `${n:-…}`.
- `bug-convert-annotate-midline` (P2): `--annotate` writes its `# was:`
  comment mid-line, which comments out the rest of the line.
- `bug-convert-test-whitespace-collapse` (P2): the converter collapses
  whitespace inside a quoted test operand and splits it at `&&` / `||`.
- `bug-convert-output-fails-to-compile` (P3): a catalogue of converter
  output that fails to compile with no `# MANUAL:` marker, including `$#`
  in a function (DD81's migration is `: @args`).
- `q-attest-shorthand-int-operand` (P4): whether `attest n -n` on an int
  stays SB4030, as D1 makes it; the attest-all form over an int array is
  unaffected.
- `bug-parse-error-in-block-cascades-sb4072` (P4): a parse error inside a
  block also reports SB4072 at the block's closing `}`.

### 6. F7 condition status classifier

- [x] **B85** (fixed-on-branch; DD55). Wildcard condition-operand scans
  settled on the preceding command: `cmd; if [[ … ]]` lost `cmd`'s abort.
  Branch specs: 55.89-55.103.
  - Landed in F7 (journal 536), as-is: one shared definition of the
    condition operand (`ssa::condition_operand_index`), used by the
    transform's uncheck and both backends' redirect guard. DD55 addendum and
    DD70 note (2026-10-26). Specs `55.89`-`55.103` verbatim, plus `55.104`
    (`if a; [[ … ]]`) and `55.105`/`55.106`, which pin the pipeline and
    redirected-call arms; a unit test pins the `$?` assignment arm.

Filed as backlog tasks during F7, under the same rule as F15's:

- `bug-match-out-array-capture-operand-ice` (P1): a capture as the subject
  or pattern of the out-array `match … m` / `matchall` ICEs ("should have
  been lifted"): a function capture with or without `--no-taint`, an
  external one under `--no-taint` (SB5020 otherwise). It is pre-existing.
  The 2-argument `match` and `$(match …)` work. Same shape as the done
  `bug-fn-capture-in-test-operand-ice`.
- `q-convert-capture-in-condition-operand` (P3): converted bash with an
  external capture in a test operand now aborts under `--no-taint` where
  bash carries on; whether the converter should flag it, rewrite it to
  `$(- …)`, or leave it.
- `q-condition-classifier-pipelinefinish` (P4): the classifier's
  `PipelineFinish` arm cannot be reached and would be wrong for a finish
  that preserves the status; ICE or precise answer. Also notes that its
  `Command` arm counts every internal sigil, as HEAD's scan did.

### 7. B55 output-path panic (sent to the backlog)

- [ ] **B55** (open; no DD). `sash -o` into an unwritable directory panics
  "write .o: Permission denied" (rc 101) at `compile_native`'s `.expect`;
  `link_object`'s `.expect("ld failed")` has the same shape.
  - Tracked by `bug-native-object-temp-file`, under the same rule as
    F15's (2026-10-26). The branch does not fix it, and no later step
    depends on it.

Filed as backlog tasks while researching B55, under the same rule:

- `bug-native-object-temp-file` (P2; B55):
  - a failed object write or ld spawn panics (rc 101) and can leave the
    object behind;
  - the fixed name `<output>.o` overwrites, then deletes, an existing file
    of that name;
  - a symlink with that name is written through.
- `bug-stdout-write-failure-panics` (P3): every stdout write in `sash` and
  `sash-expand` panics on EPIPE or ENOSPC, even after the binary is built.
  `sash-convert` already reports the error and exits 1. Question: should
  EPIPE be quiet?

### 8. F1 + F4 wrappers, operand and condition grammar

- [x] **B11** (open+question; DD38). `return N &`, `break &` and `continue
  &` run parent code in the background child (`done` prints twice). The
  branch is worse: `return 3 &` ICEs and `break &` is silently dropped.
  Question: reject (recommended, with a transform backstop) or give `&`
  subshell-exit semantics.
  - Landed in F1+F4 (journal 537): rejected, SB4024 at the control word,
    following DG's principle that reserved words mean nothing in a pipeline
    or under `&` (2026-10-26; decide-and-report). Specs 55.116-55.118. F2
    keeps the branch's transform half, which clears the child's return
    target and loop stack.
- [x] **B12** (open; DD122). Every in-process builtin except `echo`, `write`
  and `:` (`exit`, `return`, `break`, `cd`, `wait`, `readline`, …) compiles
  as a non-final pipeline stage, then dies with "bare command passed to
  sash_exec" (127). SB4022's principle, that a stage is a forked child whose
  effects are discarded, says reject.
  - Landed in F1+F4 (journal 537): every in-process builtin is SB4022 as a
    stage, final or not, except `echo`, `write`, `:` and DD122's final
    reader. The reader may carry `-`, or `!` where a condition consumes it.
    Specs pf.35-pf.66 (pf.41 is B19's, pf.52 B23's). `exec` in a forked
    child and a value intrinsic as a stage are questions in
    `q-suppression-and-stage-residuals`.
- [x] **B19** (open; DD122). `statement_form_label` calls an assignment "`((
  … ))`", so `x=safe | cat` draws an SB4022 naming the wrong form.
  - Landed in F1+F4 (journal 537): the message names an assignment as one
    (pf.41).
- [x] **B23** (fixed-on-branch; DD55). `a | b &` draws a false SB4022; DD55
  says it is legal.
  - Landed in F1+F4 (journal 537), as on the branch (pf.52).
- [x] **B24** (fixed-on-branch; DD55, DD95). A `-` operand swallowed the
  rest of the list: `/bin/true && - /bin/false || /bin/echo x` compiles and
  runs, where the unsuppressed form is DD95's SB4053. The branch rejects
  both.
  - Landed in F1+F4 (journal 537), as on the branch: a `-` operand ends at
    the next list operator, so SB4053 sees the rest (55.129), and `a && - b
    &` backgrounds the whole list, as `a && b &` does (55.131; 55.130 is
    the positive).
- [x] **B25** (fixed-on-branch; DD24, DD55). CRITICAL taint laundering: `-
  /bin/cat f | readline x` leaves `x` clean, because the source classifier
  peels neither Suppressed nor the stage marks.
  - Landed in F1+F4 (journal 537), as on the branch: the classifier peels
    `-` and the stage marks (24.21). An external producer still taints a
    `-` or negated reader (24.26, 24.27).
- [x] **B26** (fixed-on-branch; DD55). A condition whose final command is a
  background job compiles: `if - /bin/false & { echo y } else { echo n }`.
  The branch rejects it (SB4024).
  - Landed in F1+F4 (journal 537), as on the branch (55.119).
- [ ] **B27** (open; DD86). False SB5050: `collect_value_fn_operands`
  neither recurses nor peels, so a read through a nested or suppressed value
  call is not counted.
  - Also reached with no wrapper (F13 probe n2, base and F13 worktree):
    `map m=([k]=v); fn f { keys m x; echo "${x[0]}" }; f` is SB5050 "`m`
    is assigned but never read" at 1:1. The function records a bare
    operand only for a `Command` at the top level of the list it is given,
    so the same bare call nested in any body is missed.
  - Tracked by `bug-peel-family-classifiers` (filed during F1+F4, journal
    537): the branch does not fix it, and F1+F4 does not touch
    `collect_value_fn_operands`.
- [x] **B28** (open; DD71). `int n=1 | cat`, `int n=1 &` and `array a=(x) &`
  are rightly rejected, but as SB4014 "expected variable name in
  declaration" at 1:1.
  - Landed in F1+F4 (journal 537): a declaration stops at `|`, `&`, `&&`
    and `||`, as an assignment does, and gets the same verdicts
    (71.1-71.5). In a control-flow header the body's `{` ends it too, so a
    header declaration is SB4025 (71.6-71.8). Outside a header 71.9 still
    pins SB4014, which `q-statement-form-residuals` asks about.
- [x] **B31** (open; DD24, DD122). False positive: a wrapped clean-pipe
  reader, `echo hi | - read x`, is tainted, because
  `taint_stmt_no_read_taint` peels only a direct command.
  - Landed in F1+F4 (journal 537): the reader stage's `-` and `!` are
    peeled, so a clean producer leaves the reader clean (24.22-24.25,
    24.28, 24.29). A clean-fed `while readline` loop is still tainted, by
    another function: `bug-validator-taints-clean-pipe-while-reader`.
- [ ] **B42** (open; DD55, DD75). `x=$(- len a)` is a false SB5050; `$(- len
  HOME)` ICEs in native and yields garbage in C.
  - Tracked by `bug-peel-family-classifiers` (journal 537); its `len HOME`
    half by `bug-reserved-names-in-arithmetic`.
- [ ] **B43** (open; DD55). `len_cmdsub_operand` does not peel Suppressed.
  - Tracked by `bug-peel-family-classifiers` (journal 537).
- [ ] **B44** (open; DD86). A nested `keys m k` is not counted as a read: a
  false SB5050.
  - Tracked by `bug-peel-family-classifiers` (journal 537).
- [ ] **B45** (open; DD55). `join_call` ICEs on a suppressed capture.
  - Tracked by `bug-peel-family-classifiers` (journal 537).
- [ ] **B46** (open; DD55). `flag_operand_spelling` ICEs on a suppressed
  capture.
  - Tracked by `bug-peel-family-classifiers` (journal 537).
- [ ] **B47** (open; DD55). `flag_arg_builtin` does not peel Suppressed, so
  a suppressed form bypasses SB4070 and SB5038.
  - Tracked by `bug-peel-family-classifiers` (journal 537).
- [ ] **B48** (open; DD55, DD69). `x=$(- keys m)` ICEs, and iterating `$(-
  keys m)` or `$(- argv 1)` draws SB5009, because the splat recognizers do
  not peel.
  - Tracked by `bug-peel-family-classifiers` (journal 537). The branch's
    partial fix (H064, H065, H191, H197) is dropped from the port; the task
    cites it as reference.
- [ ] **B50** (open; DD55). `x=$(- sub abc b '\q')` prints `a\qc`; the
  unsuppressed form is SB4033.
  - Tracked by `bug-peel-family-classifiers` (journal 537).
- [ ] **B121** (open; DD74). A redirected predicate call escapes SB4037:
  with a void `f`, `if f >file`, `f >file || …`, `if ! f >file` and
  `if /bin/true && f >file` compile, where each unredirected call is
  SB4037. Found 2026-10-28 during F2; the same on base, branch and
  hand-parser, in both backends.
  - Tracked by `bug-peel-family-classifiers` (its B121 section).
- [x] **B112** (open; DD95). `while - { … } { … }` sidesteps SB4027, a bug
  per DD95. Fixing it rewrites the branch's endck.10 and endck.11.
  - Landed in F1+F4 (journal 537): a `{ }` group behind `-` is SB4027
    (cnd.7). W5 rewrites endck.10 and endck.11 to match.
- [x] **n1** (fixed-on-branch; DD55). `! - a | b`: SB4018 on base, SB4019 on
  the branch.
  - Landed in F1+F4 (journal 537), as on the branch (bng.13).
- [x] **n2** (fixed-on-branch; DD55). `a | - ! b`: SB4022 on base; the
  branch compiles and runs it.
  - Landed in F1+F4 (journal 537), not as on the branch: `- !` is SB4019
    everywhere (DG, 2026-10-26), here on the final stage (bng.15).
- [x] **n3** (control; DD55). `- ! /bin/false` runs on both binaries; the
  control for n1-n6.
  - No longer runs: `- !` is SB4019 (DG, 2026-10-26; bng.5, e.b70.ndash).
- [x] **n4** (fixed-on-branch; DD55). `fn f { return /bin/true & }; if f`
  runs on base; SB4024 on the branch.
  - Landed in F1+F4 (journal 537), as on the branch (55.120).
- [x] **n5** (fixed-on-branch; DD55). `if ! - a | b` runs on base; SB4019 on
  the branch.
  - Landed in F1+F4 (journal 537), as on the branch (bng.14).
- [x] **n6** (fixed-on-branch; DD55). `return /bin/false &` reached as a
  condition runs and aborts on base; SB4024 on the branch.
  - Landed in F1+F4 (journal 537), as on the branch: n4's SB4024, which
    does not depend on the producer's status, so 55.120 pins it.
- [x] **double fold** (open on base and branch; found in the port; DD55).
  A statement-level `-` folded its statement a second time, joining what
  followed its `&` to the job: `- a & && b` ran `b`, `- a & || b` and
  `- a & &` compiled (the second a job inside a job), and `- a & | b` and
  `- a & >f` reached an ICE.
  - Fixed in F1+F4 (journal 537), because parse_statement_here is a touched
    function: SB4014 after the `&`, as without the `-` (55.137-55.144).

Filed as backlog tasks during F1+F4. The rule is F15's (2026-10-25), with
DG's scope for this step (2026-10-26): a finding inside a function F1+F4
touches is fixed here, and any other finding goes to the backlog.

- `bug-peel-family-classifiers` (P1; B27, B42-B48, B50): `$(- intrinsic …)`
  and a nested `keys` reach classifiers that do not peel: ICEs, false
  SB5050, skipped checks.
- `bug-noop-drops-argument-effects` (P1): `:` drops its arguments' effects,
  so `: ${s:?msg}`, `: @{a:?}` and `: $(( 1 / z ))` never abort.
- `bug-return-status-string-operand` (P1): a string operand to `return`
  silently returns 0 (a literal ICEs natively), though DD89 records `return
  $s` as SB4030.
- `bug-fork-failure-reporting-by-form` (P2): a fork failure is reported
  three ways by form, and `!` turns one into success. It lands with B20.
- `bug-function-name-shadows-command-path` (P2): a function may be named
  like a path (`fn /bin/true`); the validator then checks the external
  program while the compiled code calls the function. Question: which names
  a function may take.
- `bug-handling-branch-drops-latch` (P2): in an if/elif/else body or an
  `&&`/`||` group, a latch read right after its write is a false SB5094.
- `bug-sb5012-sb5094-nested-body-flow` (P2): `$?` and latch reads in a
  capture body, a `-`-wrapped or negated compound, or a reader body are
  checked against the enclosing flow. Question: does a capture body start
  fresh (recommended) or inherit?
- `bug-validator-taints-clean-pipe-while-reader` (P2): a `while readline`
  loop fed by a clean pipe is a false SB5020; the SSA backstop binds it
  clean.
- `q-for-without-in-list` (P2): a `for` with no `in` list compiles and never
  runs (bash iterates `"$@"`), and `for x; in a b` is accepted. Recommended:
  reject the first, keep the second.
- `bug-arith-comma-classified-compound` (P3): an arithmetic comma list is
  called a compound command, and its `&&` verdict differs from its `!`
  twin's.
- `bug-arith-comma-nonfinal-part-unchecked` (P3): a comma's non-final part
  is checked only on an unsplit comma in a condition, so `(( a<3, a>1 ))`
  compiles, against DD#31.
- `bug-array-literal-elements-not-operand-words` (P3): an array literal's
  elements are lexed as command arguments, against DD71.
- `bug-declaration-names-unvalidated` (P3): a declaration or a `for` binds
  any word as a variable name (`str 9z=1` runs). Question: the spelling of
  a function name.
- `bug-negated-reader-loop-false-sb4073` (P3): `until ! readline x` as the
  last stage is a false SB4073.
- `bug-sb4025-advice-misdescribes-rhs` (P3): SB4025's advice calls every
  rejected capture an intrinsic or a concatenation.
- `q-arith-test-statement-status` (P3): a bare `(( 0 ))` or `(( a ))`
  statement sets `$?` without aborting, and the suite relies on it; DD#31
  accepts only a mutation as a statement, with status 0.
- `q-clause-on-next-line` (P3): an `else` or `elif` on the line after a brace
  body's `}` is SB4072 (SB5011 before F1+F4), though every other newline in
  an `if` or a loop is accepted. Recommended: accept it.
- `q-compound-as-condition` (P3): may a compound command, or a `{ }` group
  under `!`, supply a consumed status?
- `q-suppression-and-stage-residuals` (P3): the residual `-` forms, `exec`
  in a forked child, a here-string on a compound, and a value intrinsic as
  a stage.
- `bug-strace-count-injection-drifts-on-restart` (P3): the harness's, not
  the compiler's. strace's `when=N` counts a restarted clone, so pipeown.28's
  injected fault can land on the fork before its target (once in 266 stub
  runs, C mode). Recommended: reorder the spec so it cannot happen, and
  have `run_sash_under` detect an off-target fault.
- `bug-c-int64-min-literal` (P4): the C backend writes i64::MIN as the bare
  literal `-9223372036854775808`, which gcc warns is unsigned. The output is
  right today.
- `bug-sb5012-message-names-no-command` (P4): SB5012 says "no preceding
  command" when one preceded.
- `q-sb4072-closer-diagnostics` (P4): `a && fi` and `{ a && }` are SB4072 at
  the closer, while `a &&` at the end of the input is SB4056 at the
  operator; and SB4072 does not name the open block.
- `q-statement-form-residuals` (P4): a word after a complete statement (`}
  2`), a declaration beside a block (71.9), DD95's grouping, and `return {`.
- `dedupe-external-head-predicates` (P4): four functions and two name lists
  answer one question.
- `doc-keyword-lexing-claims` (P4): DD#125 and a resolve.rs comment say `in`
  and `do` lex as words outside their position; every keyword is a keyword
  token everywhere, and `expect_word` accepts eight of them as names.

Appended to existing tasks:

- `bug-parse-error-in-block-cascades-sb4072`: F1+F4's new errors cascade
  too (cnd.7, pf.36, pf.37). The cause is traced. A second shape, an error
  before a block's `{` (h1, h3, cov.13, cnd.22-cnd.24), is pre-existing;
  F1+F4 adds m4, r7 and r8 (skw.20), each a lone SB5011 on 8e28c61. A third
  option, giving `}` the stray-keyword guard, fails the task's own gate: it
  hides z1's and z2's real SB4072. Recommended, not yet ruled: brace-aware
  `recover`, then recovery inside the block.
- `bug-expr-type-int-disposition`: `exit ${n:?bad}` on an int is a false
  SB4030 through the same arm.
- `bug-digit-led-names`: a declaration binds a digit-led name until
  `bug-declaration-names-unvalidated` lands.

### 9. B37 resolution walk

- [x] **B37** (fixed-on-branch; DD23, DD24). Taint bypass: `resolve.rs`'s
  private copy of the walk skips a parameter expansion's index, `$(( ))`
  leaves and `(( ))` tests, so a command there keeps a bare head that the
  taint pass reads as clean. `b=${a[$(cat one)]:-dflt}` and `int n=$((
  ${a[$(cat one)]:-0} + 1 ))` compile on base and use the file's content
  unattested; the branch gives SB5020.
  - Write its specs with a `map` key (noted in F15, journal 534). Both
    programs index an indexed array with a capture, which DD80 makes
    SB4030. They compile only because of
    `bug-subscript-key-check-skips-positions` (rows id2 and ic9), so once
    that is fixed an indexed-array spec would pin SB4030, not the taint fix.
  - Landed in B37 (journal 538), as-is. `resolve.rs`'s private walk is
    deleted; resolution reaches each `$( … )` body through
    `Statement::cmdsub_bodies_mut`, the mutable twin of the walk that
    validate's taint and command checks and type_check use.
    `ir::cmdsub_twin_tests` pins both walks to the same bodies in the same
    order. Specs `capres.1`-`capres.20` (taint.sh) use map keys. Of the 16
    negatives, base compiled 9 silently and caught the other 7 only in the
    positionless SSA backstop; all 16 now get a positioned SB5020 or
    SB5021. DD23 addendum (2026-10-27). One visible side effect: when an
    indexed assignment's value and index both draw SB5095, the value's is
    reported first now, as SB5011's already was.

Filed as a backlog task during B37, under the same rule as F15's:

- `bug-redirect-target-index-capture-ice` (P1): a capture in a redirect
  target's subscript ICEs in both backends: a function capture
  (`>${m[$(f)]:-…}`) with or without `--no-taint`, an external one under
  `--no-taint` (SB5020 otherwise since B37; base ICE'd there too). SB4030's
  advice for the undispositioned form, `${m[…]:?message}`, leads straight
  into it. Recommended, not yet ruled: SB4049 for a `$( … )` anywhere in a
  target.

### 10. F2 forked children

DG 2026-10-28: F2 lands without the B82 wrap and without specs that pin
`-` reaching into a function (`- f |`, `- f &`); see
`q-suppression-on-user-functions`.

- [x] **B9** (fixed-on-branch+question; DD56). Every forked child (a
  function pipeline stage, a background job) runs END when it exits; inside
  END that is a fork bomb. Question: are END-forked children checked? Owed:
  DD56's addendum "END runs only in the main process".
  - Landed in F2 (journal 539): a forked child leaves through
    `Terminator::ChildExit` with its own `$?`, and `child_region` keeps its
    blocks out of END's abort routing, in both backends. DD56's addendum is
    written. Specs 56.89, 56.91-56.93.
  - The question is not ruled. Measured after F2: a child forked inside END
    is unchecked, as END is. `END { /bin/false & ⏎ - wait ⏎ echo end }`
    after `echo main` prints `main` and `end`, rc 0, no message. Open in
    `q-f2-residuals` R4.
- [x] **B10** (fixed-on-branch; DD55). A function pipeline stage's failure
  names `__compound_N` instead of the function.
  - Landed in F2 (journal 539): the stage is reported by the function's
    name, also when the status comes from a call inside it (pf.69-pf.71).
    pf.82 and pf.83 pin the two lines a failing stage body prints: the
    command, then the function.
- [x] **B17** (fixed-on-branch; DD122). A reader scope's finish runs in a
  background child: under `--no-taint`, every child of `seq 3 | while
  readline x { echo "got $x" & }` dies at runtime with "reader scopes must
  finish in nesting order", in both backends.
  - Landed in F2 (journal 539): the child leaves through `ChildExit`, not
    through the parent's reader finish (pipeown.42, pipeown.43).
- [x] **B20** (fixed-on-branch; DD55). A background fork failure is silent:
  `sash_fork`'s -1 is taken for the parent, the job never runs, `$!` is -1
  and the program exits 0 (shown by failing `clone` with EAGAIN under
  strace). Pipelines already guard it.
  - See `bug-fork-failure-reporting-by-form` (filed during F1+F4, journal
    537): the forms report a fork failure three ways, and `!` turns one
    into success. That task stays in the backlog; it did not land with B20
    (corrected 2026-10-28).
  - Landed in F2 (journal 539): the job prints `fork: error N`, sets
    status 1 and aborts unless checked or suppressed, as a failed pipeline
    construction does (DD55); `$!` reads -1. Inside END it is still silent
    (the by-form task's row e1). Specs bgfork.1-bgfork.8.
- [x] **B21** (fixed-on-branch; DD27). A self-tail function used as a
  pipeline stage draws SB5037.
  - F13 (journal 532) touched the stage route without fixing B21. Its
    looping template exposed a stage body inheriting its caller's tail
    loop: in `fn f { return g $1 }; fn g { … f … | cat … }`, `return g`
    in the stage lowered as a jump from the forked child into `g`'s loop.
    (Corrected 2026-10-28, journal 539: the ICE both backends showed there
    was B120's, from the argument `$(( $1 - 1 ))`; with an argument that
    does not read `$1`, the stage silently ran the recursion DD27 forbids,
    `g0|g1|g2`, rc 0.) Base's SB5037 there was an accident of its
    non-looping template. A function run as a stage now takes no tail loop,
    as a captured one does (B58), so the call is the recursion DD27
    rejects: SB5037 at 1:15 (`stgcyc.1`). The callee-owned stage loop
    that would fix B21 stays with F2.
  - Landed in F2 (journal 539): a self-tail function runs as a stage,
    including one also called plainly (stgcyc.2) and one in a background
    pipeline (stgcyc.3); pf.83 checks its body's failure report.
- [x] **B120** (open; no DD). Found 2026-10-27 during F2, pre-existing. A
  function stage took the caller's positional array before it lowered its
  own arguments, so an argument that reads the caller's positionals found
  no array: an ICE in both backends, and for `$#` in C a silently wrong
  number. The stage's argument setup was a private copy of the inliner's.
  - Landed in F2 (journal 539): the arguments are evaluated in the
    caller's frame (stgarg.1-stgarg.9, which fail on the pre-port binary).
- [x] **B22** (not-a-bug; no DD). Suspected loop on a pipeline-setup failure
  inside END; it does not occur, because the END region is unchecked.

### 11. F3 END-defined functions

- [x] **B8** (fixed-on-branch; DD58, DD56). The transform registers
  top-level functions only. An END-defined `f` used as `(f)` in arithmetic
  reads as a variable (0); an END-defined pipeline stage compiles to a bare
  exec and dies at runtime.
  - Ported in F3 (journal 541): `ir::Program::function_defs` feeds
    `register_functions`, and the lazy `-> int` insert is gone. Specs
    56.94-56.98 and 56.100; SB4019 and SB5041 now fire for END functions
    (DD56 and DD81 addenda). Backlog: `q-fn-definitions-in-end`,
    `bug-sb5041-text-for-arithmetic-uses`.
- [x] **B16** (fixed-on-branch; DD86). `bindings.rs` builds its function set
  from top level only, so names inside an END-defined function are not
  resolved: a false SB5066.
  - Ported in F3 (journal 541): `bindings::resolve` iterates
    `function_defs`. Spec 56.99.

### 12. F5 capture bodies are function bodies

- [ ] **B34** (question; DD86, DD24). `exec` inside a capture replaces the
  whole program: `echo before; x=$(exec /bin/echo hi); echo "after [$x]"`
  prints `before`, exits 0 and never runs the rest. The branch's SB5020 is a
  taint verdict (B114), not a rule; hand-parser compiles it with taint on.
  Question: no DD covers `exec` in a capture.
  - Until F5 unit E (journal 546) it also printed `hi`, to the real stdout,
    whenever the purity walk put the site on the buffer path; a body made
    impure by an unrelated external kept `hi` in the capture. Unit E marks
    `exec` impure, so both paths agree (cap.35, cap.36). That fixes the
    optimisation and answers nothing here.
  - Filed as `q-exec-in-capture` (P3; recommends (a) reject).
- [x] **B49** (F1; fixed-on-branch; DD55). `cmdsub_reads_whole_args` does
  not peel: `fn f { echo $(- join args) }; f a b c` is a false SB5043. Fixed
  by F5's rewrite of the signature scan (B53).
  - Stays with F5, whose B53 rewrite carries it. F1+F4 did not port
    `value_command`'s peel; its hunks H063 and H213 moved to F5 in the
    ledger (journal 537).
  - Ported in F5 unit B (journal 543): `cmdsub_reads_whole_args` finds the
    command through `ir::value_command`, so `$(- join args)` is variadic
    (`81.54`).
- [x] **B53** (fixed-on-branch; no DD). A second, forked copy of the
  function-signature scan disagreed with the first, giving a false SB5043.
  - Ported in F5 unit B (journal 543): `analyze_arity` runs the shared
    `ir::walk_scoped_stmts` / `ir::walk_own_exprs`; the private walk is
    gone. Specs `81.47`-`81.53`.
- [x] **B56** (fixed-on-branch; DD58, DD81). `-> int` functions were never
  arity-checked.
  - F13 changes where the check must go. The `fn_` template now loops, so
    an int function's self-tail call is lowered by `lower_int_tail_call`,
    not inlined through `inline_int_func`, where the branch put its check.
    The port must check arity on the tail path too. Its negative controls
    are a wrong-arity self-tail call, `return $(( (f) ))` in an `f` that
    reads `$1` (F13 probe t5), and the same call made ordinarily,
    `int r=$(( (f) ))` (t6): base and the F13 tree accept both. The string
    twin was F13's own regression, caught before commit
    (`try_producer_tail_call`).
  - Ported in F5 unit B (journal 543): `inline_int_func` checks every
    inlined call, as on the branch, and `lower_int_tail_call` checks the
    self tail call that F13's loop lowers. Specs `81.55`-`81.61`.
- [x] **B57** (fixed-on-branch; DD27). `$(f)` inside `f` overflowed the
  compiler's stack instead of drawing the recursion rejection.
  - Ported in F5 unit A (journal 542): every call route, the capture
    included, goes through `functions::reject_recursive_call` (SB5037 at
    the statement). Specs rec.5, rec.6, rec.9; DD27 addendum.
- [x] **B58** (fixed-on-branch; DD27). A captured function inherited the
  caller's tail loop: `return g` in `$(f)` inside `g` produced silent empty
  output.
  - Landed with F13, because it is coupled to the `define` ICE: without a
    callee-owned tail loop in `capture_func_into`, capturing a legal
    self-tail function that declares a `const` (`str s=$(f c)`) would ICE
    where base gave a false SB4044. Now SB5037 at the cycle's tail call
    (`capcyc.1`, `capcyc.2`). The
    same hunk's (H238) inlining push/pop is B57's and stays in F5.
- [x] **B59** (fixed-on-branch; DD27, DD86). An anonymous capture body
  inherited the tail loop the same way (`$(…; return g x)`).
  - Ported in F5 unit A (journal 542): `capture_body_into` takes the tail
    loop, so the `return g` is a recursive call, SB5037. Specs rec.7, rec.8.
- [x] **B60** (fixed-on-branch; DD86). `break` or `continue` inside `$( )`
  in a loop branched out of the open capture; all later output was lost.
  - Ported in F5 unit A (journal 542): `descend_loop_control` starts a
    `$( )` body outside any loop (SB5081). Specs cf.n19-cf.n21; DD86
    addendum.
- [x] **B71** (fixed-on-branch; DD58). The capture buffer path leaked output
  from forks run by int calls in value position.
  - Ported in F5 unit D (journal 545): `capture_body_is_pure` takes the
    value functions and checks the body of every value call it finds
    (`capture_purity.rs`, the branch's file verbatim). The calls come
    from `resolve::walk_value_calls`, a reduced H141 that walks names only.
    Specs cap.24-cap.28, cap.32 and cap.34 (the bare-name arm, which the
    branch has no spec for).
- [x] **B73** (fixed-on-branch; DD58). `type_check`'s private
  `walk_arith_funccalls` skipped Subscript and SubscriptAssign; the shared
  `ir` walker replaces it.
  - Ported in F5 unit C (journal 544): `ir::walk_arith_nodes` and
    `ir::walk_arith_funccalls` replace type_check's walker and validate's
    two private copies. Specs sx.36-sx.40. The slot-arity check on a sexpr
    call is a separate gap: `bug-sexpr-call-slot-arity-unchecked`.
- [x] **B74** (fixed-on-branch; DD86). Capture fast paths evaluated
  arguments outside the capture.
  - Ported in F5 unit D (journal 545): when a capture's arguments run a
    value call, `handle_command_substitution` lowers it as a body
    (`capture_args_run_value_calls`). A value command still takes the
    value path (`resolve::evaluates_args_in_place`). Specs cap.29-cap.33.
- [x] **B83** (fixed-on-branch; DD55, DD88). The condition context leaked
  into the bodies of functions called from a condition (named functions, int
  functions, `$(h)` inside `[[ ]]`), so their internal failures went
  unchecked. Branch specs: 55.76-55.83, 55.87.
  - Ported in F5 unit A (journal 542): `functions::lower_inlined_body`
    lowers every inlined body outside the condition context. DD55
    addendum; suppression untouched (q-suppression-on-user-functions).
- [x] **B83b** (fixed-on-branch; DD55, DD88). The same leak into anonymous
  capture bodies: under `--no-taint`, `if /bin/echo "[$(/bin/false |
  /bin/cat)]" >/dev/null { echo yes }` prints `yes`. Branch specs:
  55.84-55.86.
  - Ported in F5 unit A (journal 542): `capture_body_into` takes
    `in_condition` too.
- [ ] **Capture-hoisting order** (question; DD55, DD66, DD86; pre-existing,
  the same on the branch where it compiles). Every capture in a statement runs
  before the statement's other reads, so text order is not evaluation order.
  `⏎` marks a newline; `fn t { /bin/true }` precedes each `ev` row.
  - A name the capture writes: `echo "$g" "$(f)" "$g"`, with `f` setting
    `g`, prints `new X new`; bash's subshell gives `old X old` (probes
    dsp/o1-o7 and w1-w4, `files/p531-ledger.md`). Since F8 (B87, journal
    550) and F9 (B86, journal 551), `[[ && || ]]` operands and DD66's
    dispatch patterns are lazy on hand-parser, as on the branch: one that is
    never reached never runs. Within one operand or pattern, its captures
    still run before its other reads, and argument lists still hoist.
  - That the write happens is settled, so F5's simplification candidate
    "captures may not write outer names" is closed: DD86's 2026-09-28
    addendum (DG: changes to globals survive "unless a fork is required").
    Only the order is open.
  - `$?` to the left of a function capture: ev6 `/bin/sh -c "exit 3" || echo
    "[$?]" "$(t)"` prints `[0] `, where DD55 (`$?` is the handling branch's
    first expression) and bash give `[3]`. ev9 (`t` is `return 0`) and ev11
    (the assignment `x="[$?]$(t)"`) are the same.
  - `$?` to the right of one: ev5 `… || echo "$(t)" "[$?]"` and ev7 (in an
    `else`) print ` [0]`. They compile, though `$?` is not the first
    expression, which DD55 makes SB5012.
  - A capture body's own `$?`: ev14 `… || echo "$(echo "in=$?")" "$(t)"`
    prints `in=0`, because `$(t)` runs first (bash: `in=3`); ev13 reads the
    status `$(t)` left. Without a function capture, ev12 prints `in=3`.
    `bug-sb5012-sb5094-nested-body-flow` asks whether a body inherits `$?` at
    all; if it does not, these rows become SB5012.
  - A pure `echo` capture leaves `$?` alone: ev3 `… || echo "$(echo hi)"
    "[$?]"` prints `hi [3]`, ev10 `[3] x`.
  - On the branch, ev5-ev8, ev11, ev13 and ev14 are SB5020 (B114), so it says
    nothing about their order.
  - Options carried from the ledger: (a) keep hoisting and document it;
    (b) evaluate left to right, word by word.
  - Probes: `files/port-f1f4-probes.tar.gz`, `cb/ev1`-`ev14` (ev1, ev2 and
    ev4 use an external capture and are SB5020).
  - Filed as `q-capture-hoisting-order` (P2). Its `$?` rows are bugs under
    either option. The ab133dd2 re-probes (hn1, ev5, ev6) are in
    `files/port-f5-probes.tar.gz`.

### 13. F6 suppression scope

- [x] **B15** (question; DD55, DD74). A bare call of a status-returning
  function discards its status: `fn f { return 3 }; f; echo after` prints
  `after` and exits 0. Journal 253 accepts bare calls of void functions; no
  spec pins the behaviour. Asked in F6's discussion.
  - DG 2026-10-28: filed, unruled, with `-` on user-function calls in
    `q-suppression-on-user-functions` Q3.
  - Ruled 2026-10-30 and landed in F6 unit A (journal 547; DD55 addendum
    2026-10-30). A status function's call is handled as a condition or
    discarded with `-`; a bare call, a list tail, `f &` or `$(f)` is
    SB4016. `-` on a void function is SB4076 (new). A call is checked
    against the definition in force. Specs nonvoid.1-nonvoid.29. The
    questions left are in `q-nonvoid-classifier-residuals`.
- [x] **B76** (fixed-on-branch; DD55, DD88). In `if x=$(/bin/echo $(f))` the
  fast path lifted the inner `$(f)` out of the condition capture's
  suppression, so a `/bin/false` inside `f` aborted. DD55 (2026-10-20) and
  DD88 make a capture condition suppress the whole capture.
  - DG 2026-10-28 questions whether `-` should reach into a function at
    all; open in `q-suppression-on-user-functions` Q4 (argument captures).
  - Not a bug (F6 wrap-up, journal 549). DG's Q4 ruling (2026-10-29)
    removed the reach. A capture condition consumes the status of its
    capture's command only, so the abort above is the ruled behaviour.
    F6 unit B (journal 548) deleted `SuppressionScope`; ledger rows
    H234/H235 are dropped. On a0467a22, under `--no-taint`, the program
    aborts at f's `/bin/false` (rc 1) in both backends. No spec pinned this
    shape: noreach.13's operand capture is in a stage, so it runs in the
    stage's child, and noreach.9 is consumed by `-`, not by a condition.
    The wrap-up adds noreach.17 for it. noreach.17 is a control: the port's
    base 1309385 aborted here as well, while the branch's H234/H235 printed
    `yes [F]` and carried on (Step 0 probe vw/c1 under `--no-taint`).
- [x] **B81** (open; DD55, DD122). A command stage's argument captures run
  in the parent before the fork; both backends ICE.
  - Re-probed after B37 (journal 538): an external capture in a stage
    argument's subscript (`echo ${m[$(cat f)]:-d} | /bin/cat`) is SB5020
    now; base ICE'd on the bare `cat`. A function capture, with or without
    a subscript (`echo $(g) | /bin/cat`), and any capture under
    `--no-taint` still ICE on base, hand-parser and the branch.
  - Landed in F2 (journal 539), folded in from F6: a command stage's
    captures are evaluated in its child after its redirects, as a function
    stage's are, and a `:` stage runs its captures, as the statement `:`
    now does (stgarg.11-stgarg.20 and stgarg.22-stgarg.36; stgarg.34 and
    stgarg.36 are controls). Whether that order is right is open in
    `q-stage-capture-redirect-order`.
- [x] **B82** (open; DD55). A stage's suppression is not threaded into
  `build_function_pipeline_stage` or into B81's child; both backends ICE.
  - Not ported with F2: the trialed wrap would cement `-` reaching into a
    function (DG 2026-10-28). Open in `q-suppression-on-user-functions` Q1;
    B82 is a bug only if Q1 and Q4 keep the reach.
  - Not a bug (F6 wrap-up, journal 549). Q1 and Q4 (2026-10-29) removed the
    reach, so a stage's `-` has nothing to thread into the stage's child.
    The ICEs in B82's Step 0 probes (q6, q8) were B81's unsuppressed
    shapes, which F2 fixed. On a0467a22, in both backends, q3/q6/q8 run
    cleanly. Under `--no-taint`, `- /bin/echo $(f) | /bin/cat`,
    `- f | /bin/cat`, `- h $(( (g) )) | /bin/cat` and a middle stage
    `| - /bin/echo $(f) |` each compile. In each, the stage's child aborts
    inside f or g and reports it, the `-` discards that stage's status,
    and `done` prints. That is the ruled shape. A stage's status is its
    process's status, whatever ended the child (`q-suppression-on-user-functions`,
    2026-10-30, Q2 process forms), and a head `-` binds stage 1 only
    (`q-suppression-reach-residuals` R6). Whether a stage's `-` should
    also discard a setup failure is `q-setup-failure-suppressibility`.
- [x] **B84** (fixed-on-branch; DD55). A suppressed `a || b` reset `$?` to 0
  although `b` is unchecked. Branch specs: 55.88.
  - Ported in F6 unit C (journal 549): H219 in `lower_list_stmt`; specs
    55.145-55.147.
  - 55.88 is not ported (decide-and-report). Its premise, that the `-`
    reaches a function stage's body, was removed by DG's Q1 ruling
    (2026-10-29); noreach.6 pins the no-reach form of the same shape.

### 14. F8 lazy `[[ && || ]]`

- [x] **B87** (fixed-on-branch; DD62, DD95). `[[ a && b ]]` and `||`
  evaluated both operands eagerly. Branch specs: lazy.1-11.
  - Ported 2026-10-31 (journal 550; DD95 addendum). lazy.1-11 kept their
    ids; lazy.2 and lazy.9 quote their int operands (D1). New: tr.5, which
    pins the junction trace labels.
- [x] **B88a** (fixed-on-branch; DD93). Attest-predicate operands were
  evaluated eagerly. Branch specs: atlazy.1-4 and atlazy.7.
  - Correction: the line above used to say atlazy.1-7, but atlazy.5-6 are
    B88d's.
  - Ported 2026-10-31 (journal 550; DD93 addendum): atlazy.2, .3, .4 and
    .7.
  - atlazy.1 is deferred to F10. Its `$(( 1 / z ))` operand is SB4030
    after D1, and quoting it would pin `$(( ))` in a predicate as legal,
    which is one of F10's questions.
    - Settled by F10 (journal 552): `$(( ))` in a predicate is SB5098
      (atpred.17), so atlazy.1 is dropped.
- [x] **B90** (not-reproduced; no DD). Suspected gap in a while header's
  phi; both taint layers reject it on both binaries.

### 15. F9 dispatch

- [x] **B86** (fixed-on-branch; DD66). Dispatch built every pattern test
  eagerly; a capture in a pattern ICE'd in both backends; the subject was
  not copied before the patterns ran. Branch specs: dsplazy.1-15.
  - Re-probed after B37 (journal 538): a capture in the subject ICEs too,
    not only one in a pattern. A function capture in either
    (`[[ $(g) == k {…} || {…} ]]`, `[[ $s == $(g) {…} || {…} ]]`) and any
    capture under `--no-taint` ICE on hand-parser; the branch runs them.
    The same capture in a plain test (`[[ ${m[$(g)]:-d} == v ]] && echo a`)
    works. With taint on, an external capture in the subject's subscript
    is SB5020 now; base ICE'd on the bare `cat`.
  - Ported 2026-10-31 (journal 551; DD66 addendum). dsplazy.1-15 kept their
    ids; dsplazy.7 and dsplazy.9 quote their int operands (D1). The subject
    is evaluated once, and copied to `__dispatch_subject_N` when a pattern
    is dynamic.
- [x] **B93** (fixed-on-branch; DD66, DD24). Dispatch patterns were not
  taint-checked. Branch specs: dsptaint.1-3.
  - B37 does not reach it: an external capture in a pattern's subscript
    (`[[ $s == ${m[$(cat f)]:-d} {…} || {…} ]]`, bare `cat` or `/bin/cat`)
    still ICEs with taint on, in both backends, because nothing
    taint-checks the pattern. The branch gives SB5020 at 3:10.
  - Ported 2026-10-31 (journal 551; DD66 addendum): `taint_case_arms`
    checks each pattern with `check_expr_taint`. dsptaint.1-3 are verbatim;
    .1 and .2 are SB5020 at 2:9 and 2:23. The subscript shape above is
    SB5020 at 3:10 now, with bare `cat` and with `/bin/cat`, where the
    pre-F9 binary ICE'd.

### 16. F10 attest predicates

Carried over from F8 (journal 550): branch spec atlazy.1, whose predicate
holds `$(( 1 / z ))`, ports here once F10 rules on `$(( ))` in a predicate.
D1 makes the whole-int operand SB4030, and quoting it would pin that form
as legal.
- Settled 2026-10-31 (journal 552). DG ruled the strict rule: a predicate
  runs no code, so `$(( ))` in it is SB5098 (atpred.17). atlazy.1 is dropped.

Unit A (journal 552) landed B32, B36, B88b and B89. Unit B (journal 553)
landed B88c, B88d, B91 and B92's shared helper, with atonce.1-2,
atlazy.5/.6/.8/.9 and two new specs, atslot.7/.8.

- [x] **B32** (question; DD93). `attest x [[ -n $x && $(ok "$x") == ok ]]`
  ICEs in both backends; the branch rejects it (SB5020, SB5098). Probably
  subsumed by SB5098.
  - Subsumed 2026-10-31 (journal 552): SB5098 at 3:22, "`$(ok …)` calls
    function `ok`". The pre-F10 binary ICE'd (ssa.rs:2465). atpred.1 pins
    the same class (a function capture in a predicate); B32 has no spec of
    its own.
- [x] **B36** (fixed-on-branch; DD93, DD113). An arithmetic-conversion
  attest predicate did not check `${…}` root reads. Branch specs:
  atpred.1-16 (shared with B88b, B89).
  - Ported 2026-10-31 (journal 552; DD93 addendum). The bare names go
    through `walk_arith_var_reads`, and `check_arith_taint` checks the
    `${…}` imports at their own spans. Pinned by atpred.16.
- [x] **B88b** (fixed-on-branch; DD24, DD93). A predicate that runs code,
  `attest x [[ $x == $(/bin/echo yes) ]]`, ICEs in both backends: DD24
  forbids external commands in a predicate, but nothing enforced it. The
  branch adds a NEW rejection, SB5098 (a predicate tests values and runs no
  code), pending DG; the branch's commit message omits it. Branch specs:
  atpred.1-16 (shared with B36, B89).
  - Reimplemented 2026-10-31 (journal 552; DD24, DD63 addenda). DG ruled
    the strict rule: no `$( … )` of any kind (echo, `$(<…)`, intrinsics,
    getenv, functions) and no `$(( ))` anywhere in a predicate, and no
    call in `(( n = s ))`. SB5098 runs under `--no-taint` too. The
    branch's narrower scope evaluated echo and some intrinsics in place;
    its atpred.2 and .9-.12 pinned that and are not ported. Specs 24.16
    (assertion tightened), atpred.1, .3-.8 and .13-.31.
- [x] **B88c** (fixed-on-branch; DD93). An attest predicate was evaluated
  more than once. Branch specs: atonce.1-2.
  - Ported 2026-10-31 (journal 553; DD93 addendum). The pass block of a
    caught scalar attest (`if attest`, `attest … || X`) only retags, through
    the shared `push_attest_retag`. It no longer re-runs the aborting
    Attest. atonce.1-2 are verbatim: strace fails the second `newfstatat`,
    and one stat is asserted.
- [x] **B88d** (fixed-on-branch; DD93, DD61). The attest-all form's sibling
  of B88a. Branch specs: atlazy.5-6.
  - Ported 2026-10-31 (journal 553; DD61 addendum). In the `if` form the
    first failing cell takes the false branch, and no later cell's
    predicate runs: native `brif` to a fail block, C `break`. atlazy.5/.6
    are verbatim (`${y:?unset}` in a later cell); atlazy.7 is the all-pass
    control.
- [x] **B89** (fixed-on-branch; DD93). A tainted comparison operand inside
  an attest predicate was accepted. Branch specs: atpred.1-16 (shared with
  B36, B88b).
  - Ported 2026-10-31 (journal 552; DD93 addendum). There is no early
    return when nothing else is tainted, and a `[[ ]]` predicate goes
    through `check_test_taint`. `== $(getenv USER)` and
    `== $(</etc/hostname)` are SB5020 plus SB5098 at 2:19. Pinned by
    atpred.13-15, which assert the SB5020.
- [x] **B91** (fixed-on-branch; DD93, DD61). C's statement-form attest-all
  failure printed the value (`= 'bad'`); native does not. Branch specs:
  atlazy.8-9.
  - Ported 2026-10-31 (journal 553; DD93 addendum). C passes a null value
    to `sash_attest_fail`, so both backends print `attest: NAME failed the
    gate`. sash-rt is unchanged. atlazy.8/.9 are verbatim and red in C only
    on the pre-F10 binary.
- [x] **B92** (fixed-on-branch; DD100, DD93). A caught attest-all over a
  DD100 slot retagged the unresolved slot name. Branch specs: atslot.1-6
  (rewritten in W1 under DD127).
  - Ported 2026-10-31 (journal 553; DD100 addendum). The shared
    `push_attest_all_check` and `push_attest_retag` resolve the slot. On
    the pre-F10 binary, native ICE'd (`no codegen slot for A_3`,
    types.rs:72), and a later read of the slot was a false SSA-backstop
    rejection in both backends. atslot.7/.8 are new fill-then-attest specs,
    which DD127 keeps legal (a body attests its own taint). atslot.1-6 stay
    W1's.

### 17. W1 local checks and summaries

W1 implements DD127's name channels (globals and collection slots),
including the rule DG chose on 2026-10-31: a function with exactly one call
site may leave its own taint for the caller to attest immediately after the
call.

- [x] **B29** (fixed-on-branch; DD24). A tainted capture argument is
  reported twice, at the same position.
  - Ported 2026-10-31 (journal 555; reimplemented on base's transfer
    functions). A capture body is walked once, before its statement, so each
    read is reported once (three times before in an `arg` default), and a
    body no longer sees its own statement's assignment: `y=hi; y=$(/bin/echo
    "$y")` and `y=hi; y="$y$(/bin/echo b)"` are accepted. Specs capwalk.1-4
    and .9-.11; H282's false SB5020 is capwalk.8.
- [x] **B30** (fixed-on-branch; DD24). `$(- join a ,)` over a tainted `a` is
  caught only by the SSA backstop, with no source position; the branch
  reports a located SB5020. The one full walk of a capture body descends
  through `-`, and `check_command_reads` checks the reader there; no summary
  is involved.
  - Ported 2026-10-31 (journal 555). Specs capwalk.5-7 (a declaration, an
    `echo` argument, an array element).
- [ ] **B115** (open; DD100, DD121, DD127). An attest of a slot inside the
  function body is ignored, a false rejection on both binaries. Prerequisite
  of DD127's outward rule.

### 18. W2 stdout (T1)

- [ ] **B33** (fixed-on-branch; DD24, DD127). A user-function capture of
  external output is clean, so `y=$(f)` needs no attest when `f` runs
  `/bin/cat`. The branch's fix ("f runs a command") is too broad (B114, e5);
  T1 replaces it. DD127 records T1. DG's "no exceptions" (2026-10-23) means
  no exemption for a wrapper such as `fn g { /usr/bin/git "$@" }`: what
  decides where the attest goes is the number of call sites, not the body's
  shape. A function with exactly one call site may be attested immediately
  after its capture (DG, 2026-10-31). 15 existing specs pin today's
  behaviour; W2 re-measures each under that rule.
- [ ] **B35** (fixed-on-branch; DD24). False positive: `fn f { echo hi }; f
  | readline x` draws SB5020 on base, but DD24 calls a user-function pipe
  source clean. T1 keeps the fix, since `f` emits no external output.
- [x] **B72** (not-a-bug; DD24). A regression of the branch's own rule,
  fixed on the branch; base and branch agree.
- [ ] **B114** (branch-regression; DD24). The branch's "f runs a command"
  rule rejects a body whose external output is attested inside it (`fn list
  { printf … | while readline n { attest n …; echo "$n" } }; y=$(list)`).
  Base accepts it, and so does T1.
- [ ] **e5** (branch-regression; DD24). `fn f { /bin/false >/dev/null; echo
  hello }; str x=$(- f)`: the branch's broad rule taints `x` although no
  external output reaches `f`'s stdout. Base and T1 accept it.

### 18. W3 intra-procedural edges

- [ ] **B94** (open; DD51). `verify_ssa_taint`'s first phase is one pass
  with no fixpoint: under `--no-taint-cst`, `x=safe; if [[ $(echo a) == a ]]
  { if [[ $(echo b) == b ]] { x=$(getenv HOME) } }; echo "[$x]"` prints the
  home directory.
- [ ] **B95** (fixed-on-branch; DD24). CRITICAL: the AST taint walker
  dropped the states at `break`, `continue` and `return`.
- [ ] **B98** (fixed-on-branch; DD24). CRITICAL: `taint_for` applied the
  loop binder to the pre-loop state.
- [ ] **B99** (fixed-on-branch; DD24). CRITICAL: `taint_while`'s exit
  ignored the state after the condition.
- [ ] **B100** (fixed-on-branch; DD24, DD86). The second phase walked
  deferred function bodies against the final top-level state.
- [x] **B101** (fixed-on-branch; DD24). `lower_while` lowered the condition
  before pushing the loop stack. The transform half lands with F5's B60.
  - Ported in F5 unit A (journal 542): both hunks (H189, H190). Specs
    cf.n22, cf.n23; DD68 addendum. W3 still re-probes the walker's view
    of a condition `break` under T2/T3.
- [ ] **B102** (fixed-on-branch; DD121). A for-binder global inside a
  function is not an SB5067 site.
- [ ] **B103** (fixed-on-branch; DD100, DD121). A slot out-parameter written
  into a global is not an SB5067 site.
- [ ] **B104** (fixed-on-branch; DD100). A slot's entry taint was resolved
  through transitively bound callers.
- [ ] **B105** (fixed-on-branch; DD86). A `$( )` body that writes an
  enclosing function's name.
- [ ] **B106** (question; DD99). Conservative false rejection: a clean-pipe
  read and a value split (`read $src A`) neither taint nor clear the
  target's earlier taint (SB5088). Clearing is valid only on success.
- [ ] **B107** (fixed-on-branch; DD24). CRITICAL: for-in words are
  re-evaluated each iteration but were checked against the pre-loop state.
- [ ] **B108** (fixed-on-branch; DD100). Value-call out-parameters were
  invisible to the AST walker; only the SSA backstop rejected them.

### 18. W4 processes

- [ ] **B18** (fixed-on-branch; DD24, DD38). CRITICAL taint bypass:
  `x=$(getenv HOME); x=safe &; - wait; echo "$x"` compiles and prints the
  tainted value. The validator walks the background child into the parent's
  state, and `lower_background` has no snapshot/restore (F2 owns that half).
  - F1+F4 (journal 537) closes the spellings known: an assignment, an
    `attest` or an `argv` under `&` is SB4024, behind `-` too (55.121-55.123,
    55.126, 55.127, 55.133, 55.134). A called function's assignment, `fn f {
    x=safe }; f &`, was
    already SB5020 on 8e28c61 and still is (probe `b18/c1`). The validator
    still walks the child into the parent's state; W4 decides what that
    walk needs now.

### 18. W5 END

- [ ] **B96** (fixed-on-branch+question; DD56). CRITICAL: END's entry state
  was modelled from the main fall-through only. Question: which exits run
  END.
- [ ] **B97** (open; DD56, DD51). Native and C disagree after an abort edge
  into END: `x=safe; x=$(/bin/sh -c "echo evil; exit 3"); attest x …; END {
  echo "end [$x:!]" }` prints `[:!]` in native and `[safe:!]` in C.
- [ ] **B109** (fixed-on-branch; DD56). CRITICAL: a pending EXIT signal was
  checked too late.
- [ ] **B110** (open; DD56, DD24). Taint bypass on both binaries: `/bin/sh
  -c 'echo hi; exit 3' | readline x; attest x [[ $x == safe ]]; END { echo
  "end $x" }` reaches END through the abort edge before the attest; C prints
  `end hi` (native prints `end `, B97's divergence).
- [ ] **B111** (fixed-on-branch; DD56). A signal poll was lost, in both
  backends.
- [ ] **e7** (question; DD56, DD24). `x=$(getenv HOME); x=clean; END { echo
  "end $x" }` prints `end clean` on both binaries. Which exits END must
  assume is W5's framing question (c). Branch specs: endck.1-11.

### 19. Typing

- [ ] **B1** (open; DD100, DD93). The transform takes its types from a
  second typing walker, `infer_types`, which never got DD100 slot typing or
  DD93's attest-target `int`: `x=${A[0]}; x+=1` through a slot, and an
  attest-converted `n+=1`, concatenate (`51`) instead of adding, in both
  backends.
- [ ] **B2** (open; no DD). An SB4030 operand error inside a capture is
  reported twice: `check_stmt` walks capture bodies and
  `check_substituted_command_operands` walks them again.
- [ ] **B4** (open; DD86, DD93). An attest arithmetic-conversion target is
  invisible to the binding rules: `str n=x` then `attest s (( n = s ))` is
  accepted where plain `n=$(( 5 ))` is SB4030, and the attest followed by
  `int n=9` is accepted where a plain binding is SB5082.
- [ ] **B5** (open; DD56, DD30). `type_check` and `infer_types` never visit
  END: an END-local `int n=5; n+=1` prints `51`, and `strlen` of an int in
  END compiles.
  - Also (F15, journal 534): `END { echo ${a[foo]} }` on an indexed array
    is a native ICE and prints `x` in C, because DD80's key check never
    runs in END. `bug-subscript-key-check-skips-positions` leaves END to
    this fix.
  - Also (D1, journal 535): a whole-int `[[ ]]` operand in END still
    compiles (`END { int n=5; [[ $n == 5 ]] && echo y }` prints `y`),
    because D1's check runs in the pass that skips END. The B5 fix must
    run `check_test_operand_types` there too.
- [ ] **B6** (open; DD86). Typing depends on textual order, which DD86
  forbids: `fn f { exit $n }; int n=1` is a false SB4030; a function that
  assigns a string to a later `int` reaches a codegen ICE; a loop-carried
  copy of a later int prints `01`/`11`.
  - F13 note: F13 changes what one B6 program reports, not B6. Probe n1,
    `fn f { keys m x; echo "${x[0]}" }; map m=([k]=v); f`, is still the
    false SB4030 at 1:8 (`m` is not a map). Probe u2 is n1 plus
    `const x=1` and a read of `x`, so `keys m x` also writes a const. It
    now reports SB4044 at 1:8 instead of the SB4030, because validate's
    SB4044 runs before type_check and stops the compile.
- [ ] **B7** (open; DD74). An int-function predicate inside a capture is
  unchecked: `x=$(if f { … })` and `x=$(f && echo a)` compile, where the
  top-level forms are SB4030.

### 19. Lifetimes

- [ ] **B13** (open; DD62). A collection unassigned on some path segfaults
  (139) in both backends: under `--no-taint`, `if [[ $(argc) == 5 ]] { argv
  1 a; }; echo "[${a[0]}]"`. DD62's Model B reads an unset container as
  empty.
- [ ] **B14** (open; DD86). Function locals leak across calls in both
  backends: `fn f { if [[ $1 == y ]] { x=hi }; echo "[$x]" }; f y; f n`
  prints `[hi]` twice. DD86 makes a local's lifetime positional, so each
  call starts empty.

### 19. DD58 collisions

- [x] **B75** (duplicate; DD58). Same finding as B79.
- [ ] **B77** (open; DD58, DD125). Function names exempt every read from
  SB5031.
  - Extended 2026-10-28 in `q-suppression-on-user-functions` (Related):
    `: $(( f ))`, Q2's replacement for `- f` on an `-> int` function,
    reads a non-int `f` as 0 without calling it.
- [ ] **B78** (open; DD58). DD58's variable/function collision is checked
  only for a plain assignment.
- [ ] **B79** (open; DD58, DD55). An int function as a pipeline stage: its
  value is read as an exit status, so `fn f -> int { echo hi; return 256 };
  f | readline s` succeeds and prints `[hi]`.
- [ ] **B80** (open; DD58, DD75). `(len f)` on a function name ICEs in
  native and yields garbage in C; `$f` deserves SB4019's advice.
