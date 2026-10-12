# 498 — Two passes that disagreed about what a function is

The coverage queue's head was `build_function_pipeline_stage`, five cold lines.
I went to classify them and found a bug underneath — then a second one.

## The cold line that was not the interesting one

Four of the five are the DD#116 recursion backstop, and its own doc comment
already classifies it: SB5037 rejects recursion in `validate`, so nothing that
reaches the transform can fire it. Category 2, validate-shadowed, gets a unit
test. Straightforward.

The fifth is line 1000:

```rust
let body = state.function_bodies.get(&resolved).cloned()?;
```

The caller guards with `state.known_functions.contains(name)`. **Two different
sets.** If they can disagree, that `?` is a silent fall-through, and the caller
goes on to treat the name as an external command.

They disagree. `known_functions` is a whole-program pre-pass set up once in
`entry.rs:16`. `function_bodies` is filled at `statements.rs:151`, when the
`fn` statement is *lowered*. So a function called before its definition is in
the first and not the second.

## Two bugs

```sh
echo hi | up
fn up { tr a-z A-Z }
```

Compiles clean. At runtime:

```
sash: bug: bare command passed to sash_exec (expected absolute path)
./t: up: command failed (exit 127)
```

The compiler emitted a program the *runtime* calls a bug.

Then, looking for the same shape elsewhere:

```sh
int n=$(( (up) ))
echo $n
fn up -> int { return 7 }
```

prints **0**. Status 0, no diagnostic, wrong answer. `inline_int_func` had

```rust
None => return SsaValue::Expr("0".to_string()),
```

and the sibling comment in `expr_handlers`' nullary-sexpr arm says, of a
different cause, *"(Was silently 0 via the empty-int-func path: `$(( (x) ))` →
0.)"* The same failure mode, fixed once, reachable again by another route.

## Why neither gate caught it

SB5011 rejects a bareword that resolves to nothing — and it covers pipeline
stages; `echo hi | nosuchcmd` is caught. It does not catch `up`, because
**validate's** function table is whole-program. Validate says "that's a
function, not a command, nothing to resolve." The transform says "no body yet,
must be a command." Each pass is locally consistent and they mean different
things by the word.

That is the same shape as journal 495's five classifiers and one wrapper, one
level up: not two spellings of a predicate, but two *populations* standing for
the same concept with different fill rules.

## The rule was already written down

I did not have to decide what forward references should do. DD#81 Part A has a
rejected/kept table, and both halves of this are in it:

| rejected (SB5041) | kept |
|---|---|
| forward function call: `echo $(f); fn f {…}` | helper below caller: `fn a { b }; fn b {…}; a` |
| forward plain call: `f x; fn f {…}` | explicit external: `/bin/cat` |

and it states the mechanism: *"record each name resolved as an external; on
`fn NAME`, if `NAME` is already in that set, reject."*

So there were three sites that resolve a bareword to an external, and only one
of them recorded:

| site | on a `function_bodies` miss | |
|---|---|---|
| `cmd_handlers.rs:231` | records, emits external | SB5041 ✓ |
| `functions.rs:464` | returns `"0"` | silent wrong answer |
| `stmt_handlers.rs:760` | falls through, no record | runtime ICE, 127 |

The fix is two copies of five lines that already existed, `lowering_fn_template`
guard and all. That guard is what keeps DD#81's *kept* column working: while
lowering a function's dead `fn_` template, a not-yet-defined callee is not a
real execution-order use, so `fn a { echo hi | b }; fn b {…}; a` must still
compile. It does — spec 81.43.

## A third hole, found by the fix

With the record in place, this now rejects:

```sh
echo hi | tr a-z A-Z
fn tr { echo no }
```

which is DD#81's *first* rejected row, dynamic shadowing, in the pipeline-stage
position. It had been accepted the whole time. I did not go looking for it; it
fell out of making the stage path agree with the plain-command path, which is
the argument for fixing a divergence at the shared rule rather than at the
symptom.

## What I checked before believing it was done

Every position a user function can be called from, forward: plain command,
`$(f)` capture, argument, `[[ ]]` operand, array element, pipeline stage (first
and middle), arith sexpr. The first five already rejected. The last two were
the holes. `${q:-$(up)}` is rejected by SB4070 for an unrelated and correct
reason.

And the kept column, backward: stage call, arith sexpr, helper-below-caller in
both statement and arith form, three-deep chain, explicit `/bin/cat`.

## On the coverage queue

This came out of ranking cold lines, which is the argument for the exercise.
The line was cold because no program reaches it — and the reason no program
reaches it *correctly* is that the two programs which do reach it are both
miscompiled. A cold line is a hypothesis; this one's answer was neither "spec
gap" nor "unreachable" but "reachable only by a bug."

I have not written the `build_function_pipeline_stage` unit test yet — the
recursion backstop is still category 2 and still owed one. It goes with the
next coverage batch, against a freshly measured `coverage.db`: these edits
renumber two measured files, so the union lcov taken this session is already
stale for exactly the files it would be used to rank.

## Shipped

- `push_command_stage` records `externally_used` on the external fall-through.
- `inline_int_func` records it on the missing-body arm.
- 10 specs, `functions.sh` 81.37-81.46 — three forward positions, the dynamic
  shadow, three kept forms, and that a never-defined stage callee is still
  SB5011 rather than being swallowed by the new record.
- Sweep 8319 (predicted 8319), zero failures, **zero flips**, exactly 20 new
  ids. `mk test` 117.
