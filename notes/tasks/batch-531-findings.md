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
ported. Track W re-implements the taint checks under T1-T3. Most of
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
- `diag-elif-clause-has-no-span` (P3): an `elif` clause's parse errors
  point at the `if`.
- `spec-cov306-308-pin-the-wrong-pass` (P3): `cov.306`-`cov.308` pin
  validate's SB4044, not the transform-time gate they describe.
- `spec-harness-hides-compiler-ice` (P2): `spec/common.sh` drops a
  compiler ICE's text from `$err` in both modes (native rc 1, C rc 101),
  so a check asserting only `$rc != 0 && -z $out` passes on an ICE, and
  on a compiler hang.

### 3. F14 malformed names and arithmetic

- [ ] **B38** (fixed-on-branch+question; no DD). Invalid parameter names are
  accepted or swallowed in arithmetic and expansions: `$(( $0 ))` and `$((
  ${0} ))` print empty, where the word `${0}` is SB5017. Questions on
  `${10}` and `$-`.
- [ ] **B39** (fixed-on-branch; DD80, DD114). Index parse errors were
  swallowed. The branch re-codes 75.164 from SB4041 to SB4035, citing a DD80
  addendum that was never written.
- [ ] **B40** (open; DD125). Reserved-name reads in arithmetic escape
  SB5031: `handled_elsewhere` exempts environment, rejected and slot names.
  Same root as B41.
- [ ] **B41** (open; DD125). `${HOME[0]}` ICEs in both backends. Same root
  as B40.
- [ ] **B66** (fixed-on-branch; no DD). The SB4032 unsupported-flag message
  carries three 18-space runs (lost line continuations).
- [ ] **B70** (fixed-on-branch; DD72, DD79). A lone `$` or a nested
  substitution (`$(`, `$((`, `$'…'`) in arithmetic draws the generic SB4020.
  The branch gives SB4035 with a message naming the form, re-coding 31.46,
  31.67, 31.85 and cov.356, and makes `${}` SB4047. Its spec comments
  attribute "the `$` starts no variable" to DD72, which no DD says.

### 4. F15 regex and sed operands

- [ ] **B51** (fixed-on-branch; DD39, DD63). The runtime's regex replace on
  a null program exits 2.
- [ ] **B52** (fixed-on-branch; DD63). Regex SB5023 was not checked at every
  command depth.
- [ ] **B62** (open; DD63). An unclosed `${x:s/b/c/` compiles and runs; DD63
  says SB4032.
- [ ] **B63** (open; DD63, DD96). A `}` in a chained edit closes the scalar
  expansion: `"${x:s/a/c/,s/}/d/}"` draws SB4032 (unquoted, SB4072).
- [ ] **B64** (open; DD96). DD96's line break after `,` is rejected in the
  scalar expansion form; the cell and statement forms accept it.
- [ ] **B65** (open; DD65). A `"` in a scalar payload stops the scan
  (SB4057); DD65 makes quotes ordinary bytes inside `${…}`.
- [ ] **B67** (open; DD63). The cell form `${a[0]:s/}/x/}` truncates at the
  RE's `}` (SB4032); DD63's Path B applies to the cell sugar too.
- [ ] **B68** (open; DD63). A positional sed subject with `}` in the RE, `fn
  f { echo ${1:s/}/x/} }`, draws SB4072: the lexer's `sed_prefix_end`
  requires a letter or `_` first, while the parser's `scan_param_name`
  accepts digits.
- [ ] **B69** (fixed-on-branch; DD39, DD63). A static regex was not checked
  at compile time.

### 5. D1 int operands in `[[ ]]`

- [ ] **B113** (open; DD30, DD66). Whole-int operands in `[[ ]]` compile,
  e.g. `[[ $(( 10 / z )) == 1 ]]`. Settled (D1): SB4030 "quote it"; `"$n"`
  and in-word interpolation stay legal.

### 6. F7 condition status classifier

- [ ] **B85** (fixed-on-branch; DD55). Wildcard condition-operand scans
  settled on the preceding command: `cmd; if [[ … ]]` lost `cmd`'s abort.
  Branch specs: 55.89-55.103.

### 7. B55 output-path panic

- [ ] **B55** (open; no DD). `sash -o` into an unwritable directory panics
  "write .o: Permission denied" (rc 101) at `compile_native`'s `.expect`;
  `link_object`'s `.expect("ld failed")` has the same shape.

### 8. F1 + F4 wrappers, operand and condition grammar

- [ ] **B11** (open+question; DD38). `return N &`, `break &` and `continue
  &` run parent code in the background child (`done` prints twice). The
  branch is worse: `return 3 &` ICEs and `break &` is silently dropped.
  Question: reject (recommended, with a transform backstop) or give `&`
  subshell-exit semantics.
- [ ] **B12** (open; DD122). Every in-process builtin except `echo`, `write`
  and `:` (`exit`, `return`, `break`, `cd`, `wait`, `readline`, …) compiles
  as a non-final pipeline stage, then dies with "bare command passed to
  sash_exec" (127). SB4022's principle, that a stage is a forked child whose
  effects are discarded, says reject.
- [ ] **B19** (open; DD122). `statement_form_label` calls an assignment "`((
  … ))`", so `x=safe | cat` draws an SB4022 naming the wrong form.
- [ ] **B23** (fixed-on-branch; DD55). `a | b &` draws a false SB4022; DD55
  says it is legal.
- [ ] **B24** (fixed-on-branch; DD55, DD95). A `-` operand swallowed the
  rest of the list: `/bin/true && - /bin/false || /bin/echo x` compiles and
  runs, where the unsuppressed form is DD95's SB4053. The branch rejects
  both.
- [ ] **B25** (fixed-on-branch; DD24, DD55). CRITICAL taint laundering: `-
  /bin/cat f | readline x` leaves `x` clean, because the source classifier
  peels neither Suppressed nor the stage marks.
- [ ] **B26** (fixed-on-branch; DD55). A condition whose final command is a
  background job compiles: `if - /bin/false & { echo y } else { echo n }`.
  The branch rejects it (SB4024).
- [ ] **B27** (open; DD86). False SB5050: `collect_value_fn_operands`
  neither recurses nor peels, so a read through a nested or suppressed value
  call is not counted.
  - Also reached with no wrapper (F13 probe n2, base and F13 worktree):
    `map m=([k]=v); fn f { keys m x; echo "${x[0]}" }; f` is SB5050 "`m`
    is assigned but never read" at 1:1. The function records a bare
    operand only for a `Command` at the top level of the list it is given,
    so the same bare call nested in any body is missed.
- [ ] **B28** (open; DD71). `int n=1 | cat`, `int n=1 &` and `array a=(x) &`
  are rightly rejected, but as SB4014 "expected variable name in
  declaration" at 1:1.
- [ ] **B31** (open; DD24, DD122). False positive: a wrapped clean-pipe
  reader, `echo hi | - read x`, is tainted, because
  `taint_stmt_no_read_taint` peels only a direct command.
- [ ] **B42** (open; DD55, DD75). `x=$(- len a)` is a false SB5050; `$(- len
  HOME)` ICEs in native and yields garbage in C.
- [ ] **B43** (open; DD55). `len_cmdsub_operand` does not peel Suppressed.
- [ ] **B44** (open; DD86). A nested `keys m k` is not counted as a read: a
  false SB5050.
- [ ] **B45** (open; DD55). `join_call` ICEs on a suppressed capture.
- [ ] **B46** (open; DD55). `flag_operand_spelling` ICEs on a suppressed
  capture.
- [ ] **B47** (open; DD55). `flag_arg_builtin` does not peel Suppressed, so
  a suppressed form bypasses SB4070 and SB5038.
- [ ] **B48** (open; DD55, DD69). `x=$(- keys m)` ICEs, and iterating `$(-
  keys m)` or `$(- argv 1)` draws SB5009, because the splat recognizers do
  not peel.
- [ ] **B50** (open; DD55). `x=$(- sub abc b '\q')` prints `a\qc`; the
  unsuppressed form is SB4033.
- [ ] **B112** (open; DD95). `while - { … } { … }` sidesteps SB4027, a bug
  per DD95. Fixing it rewrites the branch's endck.10 and endck.11.
- [ ] **n1** (fixed-on-branch; DD55). `! - a | b`: SB4018 on base, SB4019 on
  the branch.
- [ ] **n2** (fixed-on-branch; DD55). `a | - ! b`: SB4022 on base; the
  branch compiles and runs it.
- [x] **n3** (control; DD55). `- ! /bin/false` runs on both binaries; the
  control for n1-n6.
- [ ] **n4** (fixed-on-branch; DD55). `fn f { return /bin/true & }; if f`
  runs on base; SB4024 on the branch.
- [ ] **n5** (fixed-on-branch; DD55). `if ! - a | b` runs on base; SB4019 on
  the branch.
- [ ] **n6** (fixed-on-branch; DD55). `return /bin/false &` reached as a
  condition runs and aborts on base; SB4024 on the branch.

### 9. B37 resolution walk

- [ ] **B37** (fixed-on-branch; DD23, DD24). Taint bypass: `resolve.rs`'s
  private copy of the walk skips a parameter expansion's index, `$(( ))`
  leaves and `(( ))` tests, so a command there keeps a bare head that the
  taint pass reads as clean. `b=${a[$(cat one)]:-dflt}` and `int n=$((
  ${a[$(cat one)]:-0} + 1 ))` compile on base and use the file's content
  unattested; the branch gives SB5020.

### 10. F2 forked children

- [ ] **B9** (fixed-on-branch+question; DD56). Every forked child (a
  function pipeline stage, a background job) runs END when it exits; inside
  END that is a fork bomb. Question: are END-forked children checked? Owed:
  DD56's addendum "END runs only in the main process".
- [ ] **B10** (fixed-on-branch; DD55). A function pipeline stage's failure
  names `__compound_N` instead of the function.
- [ ] **B17** (fixed-on-branch; DD122). A reader scope's finish runs in a
  background child: under `--no-taint`, every child of `seq 3 | while
  readline x { echo "got $x" & }` dies at runtime with "reader scopes must
  finish in nesting order", in both backends.
- [ ] **B20** (fixed-on-branch; DD55). A background fork failure is silent:
  `sash_fork`'s -1 is taken for the parent, the job never runs, `$!` is -1
  and the program exits 0 (shown by failing `clone` with EAGAIN under
  strace). Pipelines already guard it.
- [ ] **B21** (fixed-on-branch; DD27). A self-tail function used as a
  pipeline stage draws SB5037.
  - F13 (journal 532) touched the stage route without fixing B21. Its
    looping template exposed a stage body inheriting its caller's tail
    loop: in `fn f { return g $1 }; fn g { … f … | cat … }`, `return g`
    in the stage lowered as a jump from the forked child into `g`'s loop,
    an ICE in both backends. Base's SB5037 there was an accident of its
    non-looping template. A function run as a stage now takes no tail loop,
    as a captured one does (B58), so the call is the recursion DD27
    rejects: SB5037 at 1:15 (`stgcyc.1`). The callee-owned stage loop
    that would fix B21 stays with F2.
- [x] **B22** (not-a-bug; no DD). Suspected loop on a pipeline-setup failure
  inside END; it does not occur, because the END region is unchecked.

### 11. F3 END-defined functions

- [ ] **B8** (fixed-on-branch; DD58, DD56). The transform registers
  top-level functions only. An END-defined `f` used as `(f)` in arithmetic
  reads as a variable (0); an END-defined pipeline stage compiles to a bare
  exec and dies at runtime.
- [ ] **B16** (fixed-on-branch; DD86). `bindings.rs` builds its function set
  from top level only, so names inside an END-defined function are not
  resolved: a false SB5066.

### 12. F5 capture bodies are function bodies

- [ ] **B34** (question; DD86, DD24). `exec` inside a capture replaces the
  whole program: `echo before; x=$(exec /bin/echo hi); echo "after [$x]"`
  prints `before` and `hi`, exits 0 and never runs the rest. The branch's
  SB5020 is a taint verdict, not a rule. Question: no DD covers `exec` in a
  capture.
- [ ] **B49** (F1; fixed-on-branch; DD55). `cmdsub_reads_whole_args` does
  not peel: `fn f { echo $(- join args) }; f a b c` is a false SB5043. Fixed
  by F5's rewrite of the signature scan (B53).
- [ ] **B53** (fixed-on-branch; no DD). A second, forked copy of the
  function-signature scan disagreed with the first, giving a false SB5043.
- [ ] **B56** (fixed-on-branch; DD58, DD81). `-> int` functions were never
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
- [ ] **B57** (fixed-on-branch; DD27). `$(f)` inside `f` overflowed the
  compiler's stack instead of drawing the recursion rejection.
- [x] **B58** (fixed-on-branch; DD27). A captured function inherited the
  caller's tail loop: `return g` in `$(f)` inside `g` produced silent empty
  output.
  - Landed with F13, because it is coupled to the `define` ICE: without a
    callee-owned tail loop in `capture_func_into`, capturing a legal
    self-tail function that declares a `const` (`str s=$(f c)`) would ICE
    where base gave a false SB4044. Now SB5037 at the cycle's tail call
    (`capcyc.1`, `capcyc.2`). The
    same hunk's (H238) inlining push/pop is B57's and stays in F5.
- [ ] **B59** (fixed-on-branch; DD27, DD86). An anonymous capture body
  inherited the tail loop the same way (`$(…; return g x)`).
- [ ] **B60** (fixed-on-branch; DD86). `break` or `continue` inside `$( )`
  in a loop branched out of the open capture; all later output was lost.
- [ ] **B71** (fixed-on-branch; DD58). The capture buffer path leaked output
  from forks run by int calls in value position.
- [ ] **B73** (fixed-on-branch; DD58). `type_check`'s private
  `walk_arith_funccalls` skipped Subscript and SubscriptAssign; the shared
  `ir` walker replaces it.
- [ ] **B74** (fixed-on-branch; DD86). Capture fast paths evaluated
  arguments outside the capture.
- [ ] **B83** (fixed-on-branch; DD55, DD88). The condition context leaked
  into the bodies of functions called from a condition (named functions, int
  functions, `$(h)` inside `[[ ]]`), so their internal failures went
  unchecked. Branch specs: 55.76-55.83, 55.87.
- [ ] **B83b** (fixed-on-branch; DD55, DD88). The same leak into anonymous
  capture bodies: under `--no-taint`, `if /bin/echo "[$(/bin/false |
  /bin/cat)]" >/dev/null { echo yes }` prints `yes`. Branch specs:
  55.84-55.86.

### 13. F6 suppression scope

- [ ] **B15** (question; DD55, DD74). A bare call of a status-returning
  function discards its status: `fn f { return 3 }; f; echo after` prints
  `after` and exits 0. Journal 253 accepts bare calls of void functions; no
  spec pins the behaviour. Asked in F6's discussion.
- [ ] **B76** (fixed-on-branch; DD55, DD88). In `if x=$(/bin/echo $(f))` the
  fast path lifted the inner `$(f)` out of the condition capture's
  suppression, so a `/bin/false` inside `f` aborted. DD55 (2026-10-20) and
  DD88 make a capture condition suppress the whole capture.
- [ ] **B81** (open; DD55, DD122). A command stage's argument captures run
  in the parent before the fork; both backends ICE.
- [ ] **B82** (open; DD55). A stage's suppression is not threaded into
  `build_function_pipeline_stage` or into B81's child; both backends ICE.
- [ ] **B84** (fixed-on-branch; DD55). A suppressed `a || b` reset `$?` to 0
  although `b` is unchecked. Branch specs: 55.88.

### 14. F8 lazy `[[ && || ]]`

- [ ] **B87** (fixed-on-branch; DD62, DD95). `[[ a && b ]]` and `||`
  evaluated both operands eagerly. Branch specs: lazy.1-11.
- [ ] **B88a** (fixed-on-branch; DD93). Attest-predicate operands were
  evaluated eagerly. Branch specs: atlazy.1-7.
- [x] **B90** (not-reproduced; no DD). Suspected gap in a while header's
  phi; both taint layers reject it on both binaries.

### 15. F9 dispatch

- [ ] **B86** (fixed-on-branch; DD66). Dispatch built every pattern test
  eagerly; a capture in a pattern ICE'd in both backends; the subject was
  not copied before the patterns ran. Branch specs: dsplazy.1-15.
- [ ] **B93** (fixed-on-branch; DD66, DD24). Dispatch patterns were not
  taint-checked. Branch specs: dsptaint.1-3.

### 16. F10 attest predicates

- [ ] **B32** (question; DD93). `attest x [[ -n $x && $(ok "$x") == ok ]]`
  ICEs in both backends; the branch rejects it (SB5020, SB5098). Probably
  subsumed by SB5098.
- [ ] **B36** (fixed-on-branch; DD93, DD113). An arithmetic-conversion
  attest predicate did not check `${…}` root reads. Branch specs:
  atpred.1-16 (shared with B88b, B89).
- [ ] **B88b** (fixed-on-branch; DD24, DD93). A predicate that runs code,
  `attest x [[ $x == $(/bin/echo yes) ]]`, ICEs in both backends: DD24
  forbids external commands in a predicate, but nothing enforced it. The
  branch adds a NEW rejection, SB5098 (a predicate tests values and runs no
  code), pending DG; the branch's commit message omits it. Branch specs:
  atpred.1-16 (shared with B36, B89).
- [ ] **B88c** (fixed-on-branch; DD93). An attest predicate was evaluated
  more than once. Branch specs: atonce.1-2.
- [ ] **B88d** (fixed-on-branch; DD93, DD61). The attest-all form's sibling
  of B88a. Branch specs: atlazy.5-6.
- [ ] **B89** (fixed-on-branch; DD93). A tainted comparison operand inside
  an attest predicate was accepted. Branch specs: atpred.1-16 (shared with
  B36, B88b).
- [ ] **B91** (fixed-on-branch; DD93, DD61). C's statement-form attest-all
  failure printed the value (`= 'bad'`); native does not. Branch specs:
  atlazy.8-9.
- [ ] **B92** (fixed-on-branch; DD100, DD93). A caught attest-all over a
  DD100 slot retagged the unresolved slot name. Branch specs: atslot.1-6
  (rewritten in W1 under T3).

### 17. W1 local checks and summaries

- [ ] **B29** (fixed-on-branch; DD24). A tainted capture argument is
  reported twice, at the same position.
- [ ] **B30** (fixed-on-branch; DD24). `$(- join a ,)` over a tainted `a` is
  caught only by the SSA backstop, with no source position; the branch
  reports a located SB5020. W1's capture-site summary fixes it.
- [ ] **B115** (open; DD100, DD121). An attest of a slot inside the function
  body is ignored, a false rejection on both binaries. Prerequisite of T2.

### 18. W2 stdout (T1)

- [ ] **B33** (fixed-on-branch; DD24). A user-function capture of external
  output is clean, so `y=$(f)` needs no attest when `f` runs `/bin/cat`. The
  branch's fix ("f runs a command") is too broad (B114, e5); T1 replaces it.
  15 existing specs pin today's behaviour and flip with T1.
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
- [ ] **B101** (fixed-on-branch; DD24). `lower_while` lowered the condition
  before pushing the loop stack. The transform half lands with F5's B60.
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
- [ ] **B78** (open; DD58). DD58's variable/function collision is checked
  only for a plain assignment.
- [ ] **B79** (open; DD58, DD55). An int function as a pipeline stage: its
  value is read as an exit status, so `fn f -> int { echo hi; return 256 };
  f | readline s` succeeds and prints `[hi]`.
- [ ] **B80** (open; DD58, DD75). `(len f)` on a function name ICEs in
  native and yields garbage in C; `$f` deserves SB4019's advice.
