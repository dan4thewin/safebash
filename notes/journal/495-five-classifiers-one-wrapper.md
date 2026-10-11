# 495 — Five classifiers, one wrapper

Journal 494 closed a redirect bug and I wrote down what I thought the
enumeration was. The enumeration was wrong, and so the family was not closed.
This session found four more faults in it, plus a fifth that had nothing to do
with the cold line I was chasing and everything to do with the same wrapper.

The shape is always the same. A classifier asks *what kind of statement is
this?* and is handed `Redirected(…)` or `Suppressed(Redirected(…))` instead of
the thing it was looking for. Nobody wrote a rule about wrappers, so each
classifier answered the question independently, and most of them answered it
wrong.

## The one that was silent

`$(- cmd >f)` dropped its redirect. Not an error, not a diagnostic — the
filename never reached the binary, in either backend.

The parser builds that spelling as `Suppressed(Redirected(Command))`.
`single_command_subst` guarded its fast path with
`matches!(stmt.kind, StmtKind::Redirected(_))`, which tests the *outermost*
kind. The outermost kind is `Suppressed`. So the guard missed, and
`unwrap_subst_wrappers` — which peels transitively — handed back the bare
command, and the fast path built a `CmdSubst` carrying no redirects at all.

**The guard was non-transitive while the peel beneath it was transitive.**
That is the whole bug, and it is worth stating as a rule: a guard and the
walk it guards must agree about how deep they look. The guard's own comment
already stated the correct policy. It was added to fix `$(cmd >f)` and was
never extended to the wrapped spelling.

The fix makes it structurally impossible to disagree: `unwrap_subst_wrappers`
now returns `(&StmtKind, bool /* saw a redirect */)`, so the guard reads the
answer off the same walk that does the peeling.

This is the third time a fix has closed the tested spelling and left the
wrapped one live — `$(argv; touch f)` → `$(argv >f)` → `$(- cmd >f)`.

## The one the table predicted

`$(getcwd 2>/dev/null)` escaped SB4046 and drew a false SB5073.
`$(len a 2>/dev/null)` was caught correctly. Same shape, two answers.

`BuiltinCmd::from_name` maps all twelve `resolve::INTRINSICS` names to
`Other`, and `may_take_redirect` reads `Other` as "external command or user
function — allow". The redirect gate was asking an enum that cannot represent
the distinction.

`INTRINSICS`'s own doc had already written the postmortem:

> The duplication was harmless while every reader asked only "is this name the
> compiler's" […] it became a hazard the moment a name had to carry a
> denotation.

A redirect gate is exactly a reader that needs the denotation. The fix is
`resolve::head_may_take_redirect`, which joins both tables; the gate asks
that. `BuiltinCmd::may_take_redirect` stays, because it is the enum-total,
marker-enforced list where a new builtin must state its answer at compile
time — it just is not the authority for a *name*, and its doc now says so.

## The two with a private parser

Ten of the twelve intrinsics were fixed by that. `status` and `getcpid` were
not, because they never reached the gate: `parse_status_word` and
`parse_getcpid_word` claimed those words before `parse_command` ran.

Both were bespoke `Command` constructors, and both were wrong in their own
way:

- `parse_getcpid_word` discarded the rest of the command **without consuming
  it**, so `2>/dev/null` became a second statement — which made SB5073's
  complaint that this was "not one statement among several" *self-fulfilling*.
- `parse_status_word` had a private argument loop that fed `2>` to
  `parse_expr`, producing SB4014 "expected expression".

Neither is patchable in place; the bug is that the special case exists. They
now claim exactly the nullary form, by looking ahead for the closing paren,
and decline everything else. Every other spelling falls through to the
ordinary parse and collects every ordinary gate. `SpecialWord`,
`parse_status_word`, `parse_getcpid_word` and `CmdSubStep::Continue` are gone.

### And it unmasked a fifth

`$(getcpid x)` then ICEd. `value_intrinsic_arity` lists nine nullary
intrinsics and `getcpid` was not among them — the one name the table missed.
The omission had been invisible for its whole life, because the parser was
throwing the argument away before anything existed to count it.

Its comment even names the hazard it was written to prevent:

> Without an entry here an extra argument was silently discarded
> (`$(getpid X)` yielded the pid).

The table was repaired for eight siblings and the ninth was hidden behind a
private parse path. **A fix applied to a table does not reach a name that
bypasses the table.**

The `unreachable!()` it hit names `nullary_stateful_slot` as its guarantee —
the precise invariant the parser change invalidated. An ICE whose doc states
its precondition is worth more than one that does not: this one told me what
I had broken, in the panic message, on the first run.

## The one that was not about command substitution at all

Writing a spec for the above, I reached for `- cmd` then `$(status)` and got
**SB5012: no preceding command**. There plainly was one.

That turned out to be two separate questions wearing one diagnostic.

`- cmd` is correct. DD#21 makes `-` one of the three dispositions a status may
be given — checked, ignored, propagated — so this *follows*. But DG asked me
to check DD#55, and DD#55 does not leave it to be inferred:

> `$?` is not available after `- cmd` — reading `$?` after `-` is a compile
> error. You said you don't care; you don't get to inspect it.

It even names `- cmd; return $?` as a compile error specifically, which spec
`55.15` already pins. My spec premise was wrong, not the compiler. I checked a
DD before touching it, which is the only reason I did not "fix" a deliberate
rule — but I stopped at the first DD that settled the question, and the
*explicit* one was the second. **Cite the rule that states the claim, not the
first one it can be derived from.** Every reference has been moved to DD#55.

Checked the rest of DD#55 against the live binary while I was there, since it
names several of the exact spellings this session changed: `x=$(- cmd)`
captures output and does not abort (`rc=0`, `x=PARTIAL`), `x=$(cmd)` on a
failing command aborts and never assigns (`rc=7`), `- cmd; return $?` is
rejected, `cmd >f` then `$?` now reads 0 exactly as bare `cmd` does, and
`- cmd >f` then `$?` is still SB5012. All eight agree.

`cmd >f` is a bug. `leaf_status_after` asked `is_command_like`, which lists
`Command`, `Pipeline`, `List` and `ArithTest` — and not `Redirected`. So a
redirected command left the status `Undefined`, and the next `$?` was told
there was "no preceding command", naming the one thing that had demonstrably
just happened.

Its two sibling predicates in the same state machine, `stmt_backgrounds_job`
and `stmt_reaps_child`, both peel the wrapper already. The first of them
carries a doc recording this *identical* fault being found and fixed for `$!`:

> Answering it in two places is how the first cut rejected `- cmd & ; int p=$!`.

Three predicates, one wrapper, one of them never told. The fix peels
`Redirected` in `leaf_status_after` and says in a comment why suppression is
*not* peeled — so the next reader does not "finish the job" and break DD#21.

## What I got wrong, and the rule it suggests

Journal 494 says there are three bespoke `StmtKind::Command` constructors in
the parser. There are **seven**, in `parse/`:

| site | file |
|---|---|
| `intrinsic_expansion` | `expr.rs:529` |
| `build_one_sed_sub` | `expr.rs:1332` |
| `parse_status_word` | `expr.rs` *(deleted)* |
| `parse_getcpid_word` | `expr.rs` *(deleted)* |
| `parse_test_command` | `parser.rs:1412` |
| `parse_attest` | `parser.rs:2028` |
| `parse_command` | `parser.rs:2669` |

I had grepped `parser.rs` and called it an enumeration of `parse/`. **The two
I missed were the two with the bug** — which is not a coincidence, because a
construction site nobody has enumerated is exactly where an unenforced
invariant goes to die.

> An enumeration is only as good as its stated scope, and a scope you did not
> state is a scope you did not check.

The second lesson is about cold lines. Five of these faults were found from
cold `Redirected` arms. A cold arm in *one* walker is usually a totality arm.
A cold arm in a *pair of walkers that must agree* is a much stronger signal:
it means one of them is being reached by a path the other is not, and the only
reason the arm is cold is that the disagreement is doing the filtering.

## State

Fifteen new specs in `spec/errors.sh` (`rdr.32`–`rdr.46`), covering all twelve
intrinsics under SB4046, both private-parser names, the suppressed capture
honouring its redirect, the `getcpid` arity, `return CMD 2>f` versus
`return N >f` (DD#74), the appending pipeline stage, and the SB5012 fix — with
`- cmd` kept as the control that stops the fix from becoming "peel every
wrapper".

Sweep: 8257, predicted exactly. Zero regressions, thirty new ids, liveness
258.

## Still open

`if x=$(cmd 2>/dev/null) { … }` draws **SB4025 claiming it captures a
function's output**, which it does not. The pipeline spelling draws the other
SB4025, claiming it captures an intrinsic or concatenates, which it also does
not. Both messages are false; `validate::is_capture_condition_rhs` peels
`Redirected` deliberately and lets the form through, and
`transform::try_capture_condition` then does not recognise it.

Whether a capture condition admits a redirect is a language-surface question,
so it is DG's. It has to be answered either way, because fixing the drop moved
`if x=$(- cmd >f)` from "compiles, redirect silently dropped" to "rejected
with a false message", and a false message is not a resting place.
