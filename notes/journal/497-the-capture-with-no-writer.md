# 497 — The capture with no writer

The coverage queue had one item parked on DG. I went to describe it and the
description was wrong, which is how the session started.

## The item, as filed

`if x=$(cmd 2>/dev/null)` was rejected with:

> SB4025: capturing a function's output is not a condition

There is no function in that program. I had also recorded, in the filing, that
`if x=$(a | b)` failed the same way. Rather than describe it from the note I
rebuilt the surface against the live binary — and the pipeline claim was false.
Plain pipelines in a capture-condition work, and have for as long as DD#88 has
shipped. Only a redirect on a *stage* (`$(a | b 2>f)`) failed, from a different
site, with a generic message rather than a lying one.

Two defects, not one, and the smaller of them was the one I had mis-measured.

## Why the message lies

`try_capture_condition` matched two `SsaValue` shapes: a bare `CmdSubst`, and a
`CmdSubstBody` passing `is_single_pipeline`. A redirected command is neither —
`single_command_subst` declines any redirected capture, so the redirect goes
through the ordinary statement machinery and the body is not a lone pipeline.
It fell to `_ => None`.

The caller's `None` branch has exactly one explanation to offer, and it is
DD#88's: you captured a user function. So every shape that fails to match is
told it captured a function.

The two passes already disagreed, which is the part worth keeping. `validate`
*accepts* `if x=$(cmd >f)`: `is_capture_condition_rhs` peels `StmtKind::
Redirected` (validate.rs:1294) precisely so that it does. Validate said fine;
the transform said "that's a function". A fault that survives a pass only to be
mis-reported by the next one is the shape this codebase keeps producing — the
fifth classifier-disagreement in as many sessions (journal 495).

## DG's rule, and then DG's rule

I presented two options. DG:

> so long as the cmd's stdout remains connected to the caller (e.g.,
> `cmd 2>&1 >f`) it should be supported

The parenthetical is doing heavy lifting and I could not read it one way. Under
a strict fd-1 reading `2>&1 >f` is a *counter*-example: fd 1 ends up on the
file. So I measured instead of asking. A sash/bash differential:

| spelling | captured |
|---|---|
| `cmd 2>/dev/null` | `OUT` |
| `cmd 2>&1 >f` | `ERR` (and `f` gets `OUT`) |
| `cmd >f` | `` |

sash was already byte-identical to bash on all three. That settles the reading:
not "fd 1 reaches the caller" but **"the capture pipe still has a writer"** —
which `2>&1 >f` satisfies through fd 2, because fd 2 took a copy of the pipe
*before* fd 1 left for the file. Order matters, and the shell applies the list
left to right, so a static simulation answers it exactly.

Then DG broadened it:

> the idea that the capture needs a writer even after redirects makes sense for
> all captures, not just ones after `if`

Which is right, and is the difference between a special case and a rule.
`a=$(cmd >f)` is the same mistake as `if a=$(cmd >f)`. I had scoped the first
implementation to the condition — a three-state `CaptureCond` enum threaded
through the condition gate — and threw it away for a plain check in
`validate_expr`'s `CommandSubstitution` arm, which fires wherever a
substitution is written.

That is step 1 of the decision tree doing its job in the direction it usually
does not: DG's stated principle had consequences past the case that prompted
it, and the narrow reading was mine, not his.

## Decidable, as it happens

I expected to need a conservative answer for `>&$n`. There is none to give:
`reject_nonfd_dup_target` (parser.rs:3403) pins every dup target to a literal
digit run under SB4048. The simulation is total and exact — no undecidable
case, no "assume the worst" arm.

One trap: `&>file` is **one** `Redirect { fd: Some(1), op: Write }` in the IR;
the stderr half is synthesised later. So the simulation sees `>f` and rejects
it, which is correct, but it is correct by accident of representation and the
spec that pins it is worth its line.

## The permissive half

`stmt_can_write_capture` is total over all sixteen `StmtKind` variants and says
`true` far more often than `false`. A compound returns `true` — a redirect on
one branch of an `if` says nothing about the others, and SB4021 already rejects
a redirect on the compound itself. `$(a >f; b)` is legal because `b` still
feeds the capture. SB4075 fires only when *no* top-level statement can write.

Written as sixteen arms rather than a wildcard, so a new statement form has to
make the claim itself. That convention has caught three bugs in three sessions
and I am not going to be the one who breaks the streak.

## The transform needed one thing I did not expect

Relaxing `is_single_pipeline` to `is_capture_condition_body` got the redirect
to lower, and the program ran, and the exit status was wrong — the command was
still *checked*, so a nonzero status aborted instead of reaching the `else`.

`state.suppressed` is read at emission time by exactly one site,
`handle_regular_pipeline`. A plain `Command` is unchecked by a post-pass,
`uncheck_block_from`, which `lower_suppressed_stmt` ran inline. So the
capture-condition's new single-command path needed the same post-pass. I
extracted `uncheck_emitted_since` rather than write the block-range loop a
second time; this repo has been bitten twice by a private fork of a shared
walker and the second one cost a session.

The other thing that moved with the relaxation: `$(myfn)` never reaches the
body arm (it lowers to `FuncCall`), but `$(myfn 2>f)` does, because
`single_command_subst` declines any redirected capture. Relaxing the guard
without a function-table check would have silently admitted the exact shape
DD#88 exists to reject. `88.28` pins that it still earns SB4025, with the
"function's output" wording — which is now true, because the only programs that
reach it are the ones it describes.

## The specs that were right until they weren't

The sweep came in at 8299, which is what I predicted, with twelve failures:
`csr.1/2/3/7/8/9` in both modes. Those six were written *earlier this same
session* (journal 494) to pin a silent redirect-drop bug, and every one of them
asserts that `$(cmd >f)` captures empty and writes the file — precisely what
SB4075 now rejects.

Not a regression. Specs superseded by a rule adopted after they were written,
twelve hours apart.

The fix is a respelling, not a deletion: `2>&1 >f` reaches the identical
codegen — same literal or computed target, same `open`, same `dup2`, same
`CmdSubstBody` path — while keeping the capture fed. The redirect-drop bug
stays pinned by the same lines that pinned it before.

What made the twelve unambiguous was predicting 8299 first. A green sweep at an
unpredicted total tells you nothing; a *failing* sweep at exactly the predicted
total tells you the failures are behavioural and the bookkeeping is sound.

## Shipped

- `ir::redirects_preserve_capture` — the fd-table simulation, total over
  `RedirectOp`.
- `validate::check_capture_has_writer` / `stmt_can_write_capture` — **SB4075**,
  reached from `validate_expr` so it covers every `$( … )`.
- `validate::peel_capture_wrappers` — returns the innermost kind *and* each
  redirect layer, replacing a peel that discarded them.
- `stmt_is_external_command` now peels `Redirected` too — the secondary gap,
  fifteen lines below a caller that already did.
- `transform::uncheck_emitted_since`, extracted; `is_capture_condition_body`
  replacing `is_single_pipeline`.
- 18 specs (`capture_cond.sh` 88.19-88.28, cap4075.1-8), 6 respelled
  (`commands.sh` csr.*). Sweep 8299, zero failures, twelve flips — all six
  `csr` ids in both modes, recovering.
- DD#88 addendum records the rule, the table, and that rejecting
  `a=$(cmd >f)` is a deliberate divergence from bash.
