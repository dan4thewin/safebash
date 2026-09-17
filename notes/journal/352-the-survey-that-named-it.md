# 352 — The survey that named it

`opaque_form_to_string` is one of five functions in the compiler that the
entire spec suite never enters. It renders `[[ … ]]` back to text, and journal
349 closed the last path that needed it. Its caller, `expr_to_string`, runs
14574 times and has six arms that never fire — every call site passes a
literal, because SB5011 makes a command name literal or nothing.

So the coverage question was: can a non-literal reach those arms at all? I had
already learned twice (349, 351) that reading call sites gives the wrong
answer, so I probed by execution. `$c hi` — SB5011. `${c[0]} hi` — SB5011.
`strlen @s` — segmentation fault.

## Five faces of one defect

`@name` lowers to a collection traversal. A scalar subject makes the generated
code read a `char*` as a `SashTable*`. Nothing in the compiler was checking.

| program | before |
|---|---|
| `str s=hi; for x in @s { … }` | segfault |
| `str s=hi; echo $(strlen @s)` | segfault |
| `str s=hi; f @s` | segfault |
| `int i=5; for x in @i { … }` | ICE `codegen/types.rs:29` |
| `for x in @nosuch { … }` | ICE `codegen/types.rs:190` |

`echo @s` is rejected, but by SB5030 — a rule about *where* a splat may
appear, not *what* it may name. It covered one spelling by accident and told
me nothing about the subject.

## The part that stings

This is SB5068 with the operator changed. That bug — subscripting a scalar —
was found on 2026-09-10, fixed, and its postmortem opens by explaining the
shape: a polymorphic operation with three cases, two supported and the third
neither supported nor rejected. It then names the operations that survey had
covered: `len`, `join`, subscript, **and for-in**.

For-in is the splat. The inventory that found the bug listed the next one, in
writing, and it got fixed one operator at a time anyway. Seven days.

There is a general rule in that and I have written it into the task file:
**when a survey enumerates siblings, check them all in the same pass.** A
survey's value is the enumeration; stopping at the first hit throws away the
part that was expensive to produce.

## What the fix could reuse

All of it, as it happens. SB5068's postmortem argued the check belonged in
`validate.rs` and not `type_check.rs`, because a `for` variable is never in
`TypeEnv.vars` — so the scope logic had to be rebuilt in the validator
(`program_scopes`, `collect_subscriptable_names`,
`subscript_subject_is_scalar`). `check_scalar_splat` calls all three
unmodified. A fix that put its machinery in the right place made the next one
nearly free.

One asymmetry did need thought. SB5068 left undefined names to SB5031 because
SB5031 already fired there. It does not fire for splats: **SB5031 enumerates
scalar reads, and a splat is not one**, which is exactly why `@nosuch` ICE'd
rather than being caught for free. SB5079 reports undefined splat subjects
under SB5031 itself, matching `splat_not_subscriptable_diag` — which already
reports `$@[0]` that way, so the code was precedent, not invention.

Specs `75.81`–`75.89`: five rejections, four over-rejection guards. Predicted
+18 on a dual-mode file, got +18. 5396.

## A second look that came back clean

Next to the cold arm was `try_len_intrinsic`, whose `strlen` fallback runs 67
times. Its comment said the dispatch was "collection → element count,
anything else → byte length" — but SB4030 now rejects `len` on a scalar in
both spellings, so by that comment the branch should be dead. It is not.

Bug policy says stop, so I stopped: instrumented the branch, ran the native
sweep, and got the names. `args` six times, `A` through `E` and `f` fourteen
times, `s` twice. The letters are DD#100 collection-parameter slots and `args`
is the implicit per-function array — both genuine collections that are absent
from `state.collections`, so both take the "scalar" path.

They produce correct answers. `len A` on a one-element ten-byte array gives 1,
not 10; through two call levels, still 1; empty gives 0. The reason is that
`handle_param_expansion` lowers `"strlen"` to *read the var slot's length
field*, and for a collection slot that field holds the element count. The
operator name is a misnomer; the behaviour is right. The two `s` hits are the
DD#58 arithmetic sexpr `(len s)`, where a string operand is allowed and does
mean byte length.

So: not a bug, and I have rewritten the comment to say what the branch
actually does rather than what its name suggests. Worth the detour — the
comment was one step from licensing someone to delete a live branch.

## Where this leaves the coverage target

The six `expr_to_string` arms are still cold, and now I know why with
evidence: zero hits across 2791 native specs and twenty targeted probes. They
are unreachable *by validation*, not dead. The doc comment says the
exhaustiveness is deliberate — a new `ExprKind` must be a compile error there
first. Replacing them with a catch-all would gain nothing (the catch-all is
itself uncovered) and lose a compile-time guarantee.

That makes them a floor under the 100% target rather than a task, and the only
mechanism that removes them is the filed `eliminate-ice-arms-via-types` task.
That is DG's call, not mine, and I am raising it rather than quietly routing
around it.
