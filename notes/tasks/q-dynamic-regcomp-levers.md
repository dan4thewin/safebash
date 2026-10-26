---
title: "Question: should `-`, `if` and `||` catch a dynamic regex that fails to compile?"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on the question below; the ruling is recorded as an addendum to DD#70, and to DD#55 where its text changes"
  - "the ruling is pinned by specs in both backends for each lever: `-`, `if`, `||`, a `=~` test, and a capture of `sub`"
  - "SB4025's capture advice no longer says that a sash intrinsic cannot fail, or the ruling makes that true"
  - "the open `A.regcomp` line in `dd70-error-model-contract-spec.md` is closed one way or the other"
---

## What

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

## Cause

Every runtime entry point that compiles a pattern calls `sash_exit(2)` when
`regcomp9` returns null, after `regerror9` has printed the message:
`sash_regex_match`, `sash_regex_match_str`, `sash_regex_match_into`,
`sash_regex_matchall_into` and `sash_regex_replace` (sash-rt). None returns
a status, so no lever has anything to catch or discard.

## The documents disagree

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

## Open question

- **(a) Keep the abort.** A dynamic pattern that fails to compile aborts
  whatever the lever. Amend DD#55 and DD#70 to say so, keeping DD#71's
  wording. Correct SB4025's "cannot fail", and close the contract task's
  line as "aborts regardless of lever". No code change.
- **(b) Implement the levers.** A failed compile becomes a failure status
  that `if`, `||` and `-` act on. That needs three more rulings: what a
  caught or suppressed `sub` yields (the unchanged subject, as base did,
  or empty); what `$?` holds; and how `if match` tells "no match" from
  "bad pattern", since both would take the `else` branch. `e.a70.regcomp`
  would flip.

Recommendation (made during F15, not yet ruled): (a). Every regex consumer
already behaves this way, DD#39's 2026-05-24 clarification already says so,
and F15 made `sub` and `gsub` agree. Under (b), `if match`
and `match … || …` could not tell a bad pattern from a non-match, so the
catch would hide the bug it reports.

Found during the batch-531 port (step F15, finding B51). Filed under DG's
rule for the port that questions and uncovered cases become backlog tasks.

Interacts with q-runtime-syscall-layer.md (2026-10-26). Its setjmp option
(B), making the runtime's `regerror9` exit instead of returning, would
settle this question as (a) by side effect. Rule on this one first, or
rule on both together.
