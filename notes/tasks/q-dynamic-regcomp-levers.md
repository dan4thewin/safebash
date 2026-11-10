title: "Dynamic regex handling: provisionally settled by validation before use"
status: done
priority: P3
kind: question
depends: []
gates:
  - "DG's provisional validation-before-use choice is recorded in DD132 and the existing error/regex DDs are reconciled"
  - "implementation, native/C proofs, diagnostic/spec migration and evaluation are explicitly owned by q-glob-semantics, not claimed complete here"
  - "the implementation owner tracks every modified/new DD and its pre-revision baseline for coherent revision or withdrawal"

## Provisional answer, 2026-11-09

DG chose a different interface from the two historical alternatives:
`r?` tests a whole candidate, returning quiet false for invalid regex
data, and the compiler requires successful proof for every immutable
runtime pattern before use. `:!` and taint-disable switches cannot
replace proof. Regex operations then supply logical match/no-match
answers; `sub`/`gsub` have text plus a boolean, with explicit consumption
or discard and the selected sugar projection.

DD131/DD132 combine that choice with fresh immutable scratch and DG's
caught-attest rule: ordinary reads and proved regex use only in the
attest-success arm; failure and post-construct reads only as explicitly
trusted data, never trusted pattern operands. Ordinary consts retain
their independent rules. Directory selectors keep operational statuses.

The choice is **provisional**, to be implemented and evaluated on a
separate branch before DG decides what lands. This record is done
**as a policy question only**. All compiler/runtime/converter/spec work
is open in the integrated P2 `q-glob-semantics` owner, including its
exact DD revision inventory. Journal 604 records the adoption.
No implementation or new runtime measurement accompanies this closure.

That owner must cover the real CLI in both modes, distinguish invalid
candidates from no match and fatal faults, migrate changed contracts
at fresh ids and reconcile diagnostics such as SB4025's old "cannot fail"
advice. The historical `A.regcomp` contract row is transferred there,
not marked implemented by answering the question.

## Historical F15 evidence (not rerun)

DD#70 lists dynamic `regcomp` as an (A) member, "a pattern not known at
compile time can fail to compile at run time", and gives (A) two levers:
"`-` suppresses" the verdict, and "`if` / `while` / `&&` / `||` *catch* the
verdict". No lever reaches a failed regex compile. With `str p='('` as the
first line and `echo after` as the last, every program below writes
`./bin: regexp error: missing operand for (` to stderr, the same in both
backends:

| Program | Base (1309385) | After F15 |
|---|---|---|
| `- match abc "$p"` | exit 2 | exit 2 |
| `if match abc "$p" { echo y } else { echo n }` | exit 2 | exit 2 |
| `match abc "$p" \|\| echo caught` | exit 2 | exit 2 |
| `str x=abc` / `[[ $x =~ $p ]] \|\| echo caught` | exit 2 | exit 2 |
| `str x=abc` / `- [[ $x =~ $p ]]` | exit 2 | exit 2 |
| `array a=()` / `- matchall abc "$p" a` | exit 2 | exit 2 |
| `str y=$(sub abc "$p" z)` / `echo "after [$y]"` | prints `after [abc]`, exit 0 | exit 2 |
| `str y=$(- sub abc "$p" z)` / `echo "after [$y]"` | prints `after [abc]`, exit 0 | exit 2 |

No exit-2 row prints `after`. The last two rows are F15's change: `sub` and
`gsub` used to report the error and return the subject unchanged, and now
abort like every other regex consumer (DD#39's addendum of 2026-10-25). F15
ported that change from the batch-531 branch, which gives the "After F15"
column in every row.

`if y=$(sub abc "$p" z) { … }` is SB4025 at 2:4. Its advice says the
capture is of "a sash intrinsic/builtin whose result is a pure function of
its arguments and so cannot fail". With a dynamic pattern it can fail, and
it aborts.

Two specs already pin the abort: `e.a70.regcomp` (spec/errors.sh) under
`if` (`p="[bad"`; `if match hello "$p"`; asserts a nonzero exit and
`regexp error`), and `39.56` (spec/regex.sh) under `-` with `sub`.

## Cause at the recorded baseline

Every runtime entry point that compiles a pattern takes a terminal error
path when `regcomp9` returns null, after `regerror9` has printed the message:
`sash_regex_match`, `sash_regex_match_str`, `sash_regex_match_into`,
`sash_regex_matchall_into` and `sash_regex_replace` (sash-rt). None returns
the compilation failure as a status, so no lever has that failure to
catch or discard. The table above is historical F15 evidence, not a new
runtime measurement for DD131/DD132.

## Document conflict recorded during F15

These are historical quotations, not the current reconciled contracts:

- DD#55: "Failure = nonzero exit or runtime error (e.g., `regcomp` on
  dynamic pattern). `- cmd` suppresses." Its list of checked operations
  includes "Runtime calls that can fail on data (`regcomp` on dynamic
  patterns)".
- DD#70 (A), quoted above, puts dynamic `regcomp` under the levers.
- DD#70 (B) calls `match SUBJECT PATTERN` "a *pure, in-process logical
  result* — there is no syscall and nothing the runtime can 'fail' at",
  which holds only for a static pattern.
- DD#71 names no lever: a dynamic pattern "defers to the runtime regcomp,
  whose failure aborts via the DD#55 (A) channel. Validate-when-static /
  abort-when-dynamic." The (A) channel it names is the one DD#70 gives
  levers, so it can be read either way.
- DD#39's clarification of 2026-05-24 says only that it aborts: a dynamic
  pattern that fails regcomp9 "aborts with exit code 2", which it calls
  consistent with DD#55's error model ("failure = abort"). It names no
  lever.
- `dd70-error-model-contract-spec.md` is marked done, but its `A.regcomp`
  line is still unchecked.

## Superseded alternatives and selected mechanism

F15 offered blanket abort regardless of handling, or consumer-side
compile-error statuses with a separately chosen failed-substitution
value. The AI recommended blanket abort. DG did not select that
recommendation; the current DD55/DD70/DD130 recoverability contract
was not grounds to make invalid data fatal for lack of a good API.
DD132 now chooses validation before use, not an empty/unchanged-text
fallback for malformed regexes. No-match substitution text is a valid
result for a valid pattern, a different case.

Found during the batch-531 port (step F15, finding B51). Filed under DG's
rule for the port that questions and uncovered cases become backlog tasks.

DG selected **jump support for both x86_64 and AArch64 Linux** on
2026-11-10 (DD129, journal 613). `cross-compile-and-setjmp-on-mac.md` owns
that regex-local `setjmp`/`longjmp` support; the proposed ordinary-return
rewrite was not selected. No particular jump-code provider was chosen.

Invalid candidates must still return quiet false from `r?`, so a
blanket terminal `regerror9` remains incompatible with the provisional
model. The jump and the caller's reporting/result policy are separate:
keeping the former neither supplies quiet validation nor makes resource
faults recoverable. The integrated trial retains that implementation.
