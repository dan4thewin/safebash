# 535 - An int is not a string

2026-10-25. D1 is the step that was settled before the families: a whole-int
operand of `[[ ]]` is SB4030 (B113). Every one compiled on base, on the
branch and at `bf48bec`. DG saw `[[ $(( 10 / z )) == 1 ]]` in the branch's
specs: *"the $((...)) is an integer expression and shouldn't undergo any
automatic atoi within [[ ]] - I would expect an error."* DD#30 already says
so ("no implicit conversion"), and the assignment checker already rejects
`str s=$n` with the same code.

## The rule

A whole-int operand is rejected wherever `[[ ]]` compares strings:

- every operand of every test operator, unary and binary, through `!`,
  `&&` and `||`;
- the DD#66 dispatch subject and each of its patterns;
- `attest` predicates, including DD#93 Part B's shorthand, since
  `attest n -n` builds `[[ -n $n ]]`.

The message names the context and gives the fix:

- "int value in a `[[ ]]` test, which compares strings — quote it to
  convert int to string: `"$VAR"`, or compare numbers with `(( … ))`";
- "… in a `[[ ]]` dispatch, which matches strings — quote it …";
- "… in an `attest` predicate, which tests strings — quote it … (a
  shorthand such as `attest n -n` is written out in full:
  `attest n [[ -n "$n" ]]`)".

It is reported at the operand. `expr_type` decides, so the rule agrees with
the assignment checker: an int variable or cell, `$(( … ))`, `$(f)` of an
`-> int` function, `$$` and `$!` are ints; `"$n"` and `x$n` are strings and
stay legal. validate runs first, so `-eq` and its family are still SB5019
whatever their operands.

The code is one new pass in `type_check.rs`, run through `run_stmt_pass` so
that it sees final types, each function's slot scope, and every capture body
once. It walks operands with the shared `ir::walk_test_operands`, so there is
no private test walker to drift. The driver's comments had two passes
labelled "Sixth"; they now run First through Eighth, with this one Seventh.

Two gaps remain, both filed. END is not type-checked at all (B5, typing
family), so an int operand there still compiles. And `expr_type` types
`${n:?msg}` on an int as a string, so that form slips past this rule and the
assignment checker alike.

A `for` variable over an int array, and an attest-all binder, are typed
string (probes q5, q6 and q9–q12: `$(( e + 1 ))` is SB4030 "bare string
`e`", and `e+=1` appends). So `for e in @a; do [[ $e == 1 ]]` compiles,
consistently with how those binders are typed everywhere else.

DD#30 and DD#66 have addenda dated 2026-10-25. DD#66's numeric dispatch is
unchanged for a string `n`; an `int n` is written `[[ "$n" == 1 { … } ]]`.

## Fifteen specs rewritten

Each keeps its id and its assertion; only its program changed. The rule:
quote when the spec is about something else, use `(( ))` when the
comparison is numeric.

- Quoted (10): `cf.w2`, `cf.u3`, `cov.154`, `42.7`, `42.8`, `ice.3`,
  `spv.4`, `atint.5`, `atint.6`, `atint.8`.
- Rewritten to `(( ))` (5): `cov.331`, `cov.333`, `cov.334`, `42.9`,
  `spv.1`. `cov.334` tested `[[ $r != -* ]]`, so it is now `(( r >= 0 ))`.

Two comments needed more than a quote:

- `cov.154`'s comment claimed `[[ $$ == 0 ]]` reaches `skip_dollar_var`'s
  special-character arm. No `[[ ]]` operand does: that scan's only caller is
  the brace-less disposition operand, and `vm.bl22` covers the arm. The
  comment was false before D1. The id stays, quoted, with a corrected
  comment; retargeting it would only duplicate `vm.bl22`.
- `ice.3` targets the `-> int` capture arm. `"$(n)"` is a single-part string
  whose value lowers to the same `IntFuncCall`, which `lift_test_captures`
  still lifts. The spec prints `T`, the return value, not `n`'s empty stdout.

## The converter

Bash has no such rule, so D1 broke `sash-convert`: every converted test on
an int now failed to compile. That is a regression caused by this step, so
fixing the converter was part of it (DD#57 addendum, 2026-10-25).

The converter now quotes each whole test operand that the converted script
holds as a sash int:

- `$(( … ))`;
- `$?`, `$$`, `$!` and `$#`, which convert to `$(status)`, `$(getpid)`,
  `$(getcpid)` and a count;
- a length `${#…}`;
- a read of an int-typed name.

Operands of `-eq` and its family are left alone, because those tests become
`(( ))`. A reversion stub shows that exclusion is live.

Int-typed names come from a pre-pass that over-approximates by name:
`declare -i`/`local -i`, every arithmetic write target (including a C-style
`for` variable), and, to a fixpoint, every copy of an int expansion
(`rc=$?`, `q=$p`). Over-approximating is safe: quoting a whole operand never
changes what a sash test means, and a name the pre-pass misses is a loud
SB4030, never a silent change. The `[ ]` → `[[ ]]` quote-stripping keeps the
quotes on an int-typed name.

Quoting exposed two pre-existing faults that had to be fixed with it. The
converter treated a C-style `for` body, and anything in a command
substitution inside arithmetic, as arithmetic. So a `$?` there became the
DD#58 sexpr `(status)`, and a string read gained an integer abort
disposition. Unquoted, `[[ (status) == $e ]]` is a loud SB4068. Quoted, it
compiles, compares `$e` with the literal text `(status)`, and prints
`differs`. Quoting without the boundary fix would have turned a loud failure
into a silent one. Now only a `for` header is arithmetic, and a capture's
body never is. (HEAD's output for the C-for spec 57.188 was already broken
for this reason: SB4068 on both compilers.)

`--annotate` writes its `# was:` comment after the added closing quote, so the
comment cannot become part of the operand. A mid-line annotation still
comments out the rest of its line; that is pre-existing and filed.

## Specs and proof

46 new ids, all first-ever:

- `types.sh` `30.50`–`30.65`, dual-mode (`plan` 430 → 446). There are 13
  rejections: a binary, a unary and an `&&` operand, `$(( ))`, `$(f)`, a
  cell, `$$`, a dispatch subject and a pattern, a full and a shorthand
  attest, a function body, and `30.62`, a test in a capture that is itself
  an operand, reported once. There are 3 legal controls: `"$n"`, `x$n`,
  and a quoted dispatch.
- `convert.sh` `57.178`–`57.191`, native-only (`plan` 202 → 216): 12 pin
  the converter fix, and `57.179` and `57.191` are controls.

On the HEAD binaries, exactly the 13 rejections and the 12 converter specs
fail, and the controls pass. Each reversion stub turned red exactly the
specs it should:

| Stub | Reverts | Red specs |
|---|---|---|
| off | the pass | 30.50–30.62, both modes |
| twice | runs the pass twice | 30.62 only |
| hook | the quoting hook | 57.178, 57.180–57.181, 57.183–57.190 |
| strip | the strip guard | 57.182, 57.190 |
| typed | the int-typed pre-pass | 57.183–57.186, 57.188, 57.190 |
| eqx | the `-eq` exclusion | 57.191 |
| cfor | the C-for boundary | 57.186, 57.188 |
| cmdsub | the capture boundary | 57.189 |
| annot | the annotate guard | 57.187 |

`twice` was needed because HEAD raises nothing, so it cannot show that
`30.62`'s "reported once" assertion can fail. It also shows that nothing
downstream deduplicates.

## Gates

- `mk -p 1 test`: 225/27/20.
- Full sweep at `bf48bec` with D1 uncommitted:
  - 9571 = 9558 passing + 13 skips (native 4895, c 4676), which is F15's
    9525 plus 46;
  - exactly the 46 predicted first-ever ids, no flips and no retirements;
  - the can-fail query reports 260.
- The SB census is empty in both directions. `plan` matches `chk` (but for
  `coverage.sh`'s known +2), both collision checks are empty, and the new ids
  sit above the high-water marks (57.x was 177, 30.x was 49).
- The pre-commit hook passes with no bypass, after two fixes. The
  complexity gate rejected the new pass: cognitive 11, where the threshold
  was then 4.57, three times the codebase mean (its cc of 10 was under
  10.44). It is now a flat dispatcher, `check_test_operand_types` at
  cognitive 1, over helpers none above cognitive 3. And `cargo fmt`
  reflowed three spots in `convert.rs` and one in the refactored pass.
- After the refactor, `mk test` (225/27/20) and a second full sweep repeat
  the result: 9571 = 9558 passing + 13 skips, no first-ever ids, no flips,
  no retirements, can-fail 260.

## Backlog

Under DG's rule, eight tasks:

- `bug-fresh-clone-rustix-runtime-private` (P1). No `Cargo.lock` is tracked
  and both crates ask for `rustix = "1.1"`. rustix 1.1.5 made its `runtime`
  module private, and sash-rt calls three functions from it, so a fresh
  clone fails to build sash-rt (E0603). The machines here have 1.1.4
  locked. Remedies: pin `=1.1.4`, track the lockfiles, or own the three
  syscalls. DG's call.
- `bug-readline-into-int-ice` (P2). `readline` into an int ICEs in native
  codegen and emits C that gcc rejects. This is the same on base, the branch
  and HEAD.
- `bug-expr-type-int-disposition` (P2), the `${n:?msg}` gap above.
- `bug-convert-annotate-midline` (P2).
- `bug-convert-test-whitespace-collapse` (P2). Rewriting a test re-joins
  its tokens with single spaces, so `[ "$x" = "a  b" ]` compares against
  `"a b"`.
- `bug-convert-output-fails-to-compile` (P3), a catalogue of conversions
  that fail loudly, all pre-existing.
- `q-attest-shorthand-int-operand` (P4). Should `attest n -n` quote its own
  operand, rather than be rejected on an int?
- `bug-parse-error-in-block-cascades-sb4072` (P4). A parse error inside a
  block also reports a false "`}` closes nothing".

`converter-case-to-dispatch` gains a note: when `case` is converted, an int
subject must be quoted.

## Corrected along the way

Three of my working claims were wrong, and the task files carry the
corrected ones:

- `$#` in a converted function without `@args` is SB5043 by DD#81's
  2026-09-28 addendum, which is deliberate; the converter should emit
  `: @args`. The doc table in `convert.rs` that maps `$#` to `$(len args)`
  is stale.
- Probe w4 (one leading space) is not a control for the whitespace
  collapse: a single blank survives anywhere only because the re-join uses
  one space. Two leading spaces collapse.
- The readline ICE makes the compiler exit 1, not 101.

## Next

F7, the condition status classifier (B85), when DG says go.
