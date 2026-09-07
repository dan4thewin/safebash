# 251 — DD#94: newlines, `;` and comments as trivia inside `(( ))` and `[[ ]]`

**Branch:** `hand-parser`. Closes the deferred task
`arith-block-separators-and-comments.md`, plus DG's addition of dispatch blocks.

## What DG asked for

"Let's tackle the task, and address line breaks and comments within a dispatch
block." The task was a decision task with three options apiece, so I put the
options to him with worked examples first; he took the full permissive package
(1b + 2b + 3b + dispatch + multi-line tests).

Showing examples before implementing was the right call — writing them out is
what surfaced the "a comment eats the closing `))`" consequence, which turned
out to hide a real bug (below).

## The idea

sash already treated newlines and comments as trivia in array literals,
function/arm bodies, and after a pending `|`/`&&`/operator. Three holes forced
one-liners. Surveying it first made the case obvious — and turned up two
outright inconsistencies:

- a comment worked inside a `[[ ]]` **test** but not between **dispatch** arms,
  though both are `[[ … ]]`;
- an array literal could span lines but an arm list could not.

So the change isn't "be more permissive", it's "finish a rule that was already
most of the way there".

## What landed

**Arithmetic.** A standalone `(( ))` is a sequence block; `,`, `;` and newline
all sequence (value = last). The continuation rule is what makes newlines safe,
and it fell out of a single placement decision: the *operand* position skips
separators, so a pending operator swallows them (`(( 1 + ⏎ 2 ))` → 3, as
before) while a completed expression ends at one. A separator is *required*
between expressions, so `(( 1 2 ))` stays the error it always was.

The C-style for-header is untouched — `parse_c_style_for` splits the raw header
on `;` before the arith parser ever runs, so `;` keeps its slot-delimiter role
there.

**Comments in arithmetic.** `#` is a comment when whitespace-preceded, and
still the DD#72 base-literal reject when alnum-adjacent, so `16#ff` remains a
loud SB4035. That adjacency test is the entire safety argument: DD#72 exists
because `base#digits` used to truncate silently, and nothing here brings that
back.

**`[[ ]]`.** Arms one per line with comments between them, and tests spanning
lines around `&&`/`||`/operands. The skipping is at the parser's structural
points, **not** the scanner: an arm body is an ordinary statement list where
newlines still separate statements. A newline also ends one arm's pattern list,
so `foo ⏎ bar { … }` is two arms — which is what keeps a multi-line arm list
unambiguous.

## The bug this uncovered

Spec `nl.r10` (`echo $((5 # 3))` must be a clean error) went red. The cause was
not the comment change as such: with ` # 3))` now a comment, the closing `))`
was eaten, and `collect_arith_content` **returned successfully at EOF with
parens still open**. So the program compiled and printed `5` — a silent
truncation of the expression.

That was already true before this work for any unterminated `(( ))`; comments
merely made it reachable from ordinary-looking code. It is exactly the failure
mode DD#72 was written to prevent, one level up (the delimiter rather than the
literal), so an unterminated arithmetic is now a hard error that names the
likely cause:

```
error [SB4020] unterminated arithmetic — expected `))`. Note a `#` comment runs
to the end of its line, so a `))` after one is part of the comment
```

## janet gap (pre-existing)

Spec 94.10 pins that the for-header is unaffected; it fails in janet with
`unknown symbol i`. Verified pre-existing by stashing the source changes and
rebuilding — the clean tree gives the identical error, and the comma path was
untouched. janet doesn't emit bindings for a comma-sequenced for-init
(`for (( i = 0, j = 10; … ))`); a single-assignment init is fine. Filed
`janet-for-init-comma-sequence` (P3); 94.10 gated `native,c`.

## Verification

3-mode sweep: **0 regressions**, +56 passing (4274 → 4330; failing back at the
34 baseline). 19 new specs — `arithmetic.sh` 94.1–94.10 and `dispatch.sh`
94.20–94.28 — written before the implementation, and deliberately including the
guards that must NOT change: the continuation case, `16#ff`, the for-header,
one-line pattern lists, and arm bodies still separating statements on newlines.

Two of my first spec drafts were wrong rather than the compiler: `a=0` makes a
string variable so `(( a = 1 ))` is a type error (needed `int a`), and the
janet for-init gap above. Worth noting only because both looked like failures
of the feature at first glance.

## Complexity

`parse_dispatch` (cc 13) and `tokenize_arith` (cog 7) tripped the pre-commit
gate; both came back under it by extraction rather than a bypass —
`parse_dispatch_arms` takes the arm loop (6/2 + 9/3), and `push_arith_token`
takes one tokenizer step so the loop itself is trivial (2/1 + 5/4). Behavior
unchanged; the sweep was re-run afterwards.

## Follow-ups

None from the task — it is fully resolved and marked done. The DD records the
deliberate bash divergence.
