# 510 — The type checker did the proof

2026-10-15. Follows journal 509. DG: *"address C and the 40%"*.

## What landed

25 of the 27 functions in `parse/parser.rs` that returned
`Result<Option<Statement>, Diagnostic>` now return `Result<Statement,
Diagnostic>`. `parse_statement` keeps its `Option` — it is the real producer,
and three live callers consume its `None`. `parse_list_operand` keeps its own,
because its `at_eof()` arm is what `parse_trailing_pipeline` turns into SB4056
for a dangling `/bin/true &&`.

All **eight** cold lines the task filed are gone — 40% of `parser.rs`'s cold
cohort — not converted to ICEs but deleted, because the type now says what the
`unreachable!()` would have said. Among them the third unreachable user-facing
diagnostic, `return_producer_error` at `parse_return`.

Net −24 lines. One new private function, `parse_statement_here`: "parse a
statement the caller has already established is there." `parse_statement` is
now a four-line wrapper around it that does the EOF check and `.map(Some)`.

## The part worth keeping

The task file said the other seven lines were "a further step", to be taken
once `parse_return:1252`, `parse_negated:1260` and `parse_suppressed:1341` had
each been probed — because a cold line is a hypothesis and the relay chain
might be live.

That was the wrong instrument. **Narrowing a signature is self-proving.** If
any of those relays could carry a live `None`, the narrowed function would not
typecheck: the `None` would have nowhere to go and `rustc` would say so, at the
exact line. So the experiment and the fix are the same edit, and its result is
not corroboration — it is a proof, checked mechanically over every path at
once, including the ones a probe would have missed.

The evidence for that is what the compiler did with the cohort. I narrowed 25
and it accepted all 25; I left two and they are *exactly* the two with a live
`None`. The type system partitioned the group correctly without being told
which was which. Compare the probing route: three separate reachability
experiments, each needing a positive control (journal 474), each answering for
one line.

The general form, worth remembering next time a cold arm is a `None`/`Err`
relay rather than a `match` arm: **where the cold line is a type's fault,
change the type and let the compiler enumerate.** An ICE asserts "this cannot
happen" and has to be believed; a narrowed signature makes it *not expressible*,
which is the stronger claim and the cheaper one.

It also disposes of `parse_brace_else`'s `unreachable!()` from 509, which was
the one ICE in this codebase that could never be unit-tested — reaching it
required editing `parse_if`. Under option A that line was cold permanently,
outside the spec ∪ unit-test union that defines the 100% target. It is now not
a line.

## A message is not a site either

`return_producer_error` has two call sites raising *byte-identical* text
(SB4014, "return: expected a status producer"). `parse_return_value_form` is
warm — `fn f() { return - }` fires it. `parse_return` was cold. Probing the
message produces the sentence and looks like proof the cold arm works.

AGENTS.md's existing rule is "a diagnostic CODE is not a site, and `$err ==
*SB4014*` does not pin one". This is the sharper version: when a diagnostic is
built by a shared constructor, **the message does not pin the site either**.
The test is the constructor's call-site count, not the string's uniqueness.

## Verification

Sweep **8377** — 4073 c + 13 skipped + 4291 native. Predicted 8377 before
running (this is behaviour-preserving, so zero new ids was the prediction, and
zero new ids is what came back). Zero failures, zero flips. Liveness 258, so
the gate could still have fired.

`mk test` 125 + 27 + 20, unchanged — nothing here was unit-tested and nothing
needed to be.

Probed by hand, all identical to pre-refactor: empty file, comment-only file,
`;;;`, `!` at EOF (SB4014), `-` at EOF (SB5011), `- cmd`, `! cmd` (SB4018),
`return` bare, `return -` (SB4014 from the surviving site), `return a | b`,
`&&` at EOF (SB4056), `|` at EOF (SB4056), `else if`, empty `while` body.

## The gate caught a free rider

The commit tripped the complexity gate on `parse_attest` (cc 12 / cog 6
against 3×mean of 10.92 / 5.04) — and my edit to it was *two tokens*: the
signature, and one `Ok(Some(x))` → `Ok(x)`. Neither moves either metric. It was
a pre-existing violator that a mechanical touch made visible.

That is the grandfather-on-modify mechanism working exactly as designed, so I
fixed it rather than bypassing. The three-way `if`/`else if` chain in the middle
("which spelling of the attest predicate is this?") is a clean seam: extracted
as `parse_attest_predicate`, which answers `Option<Expr>` because *absence* is
a `validate` question, not a parse one. `parse_attest` 12/6 → **8/4**; the new
function is 7/4. Both clear the bar without an annotation.

Worth noting as a cost of the mechanism: a 25-signature sweep will drag in
whatever grandfathered functions it brushes against. One here, and it was a
ten-minute extraction, but a wider sweep could surface several at once.
